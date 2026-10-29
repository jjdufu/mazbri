//! 刷新令牌换发访问令牌：除校验令牌本身外，每次都按数据库当前状态复核账号。
//!
//! 刷新令牌有效期远长于访问令牌（默认 7 天）。如果只看令牌签名与黑名单，
//! 账号被禁用、锁定、要求改密、密码过期或改密、以及签发后 MFA 状态变化之后，
//! 旧会话仍可以持续换发新的访问令牌，管理员的处置要等到刷新令牌自然过期才生效。
//!
//! 刷新令牌在无 MFA 登录、MFA 验证通过（`verify`）以及强制 MFA 启用
//! （`required-enable`）后签发。已绑定 MFA 的会话可以正常刷新；仅当令牌签发后
//! MFA 因子状态发生变化（`mfa_changed_at` 晚于令牌 `iat`，即启用、关闭或超管重置）
//! 时拒绝刷新，迫使重新走完整登录。账号级 `mfa_required` 不单独阻断刷新。

use crate::audit;
use crate::error::ApiError;
use crate::middleware::admin_token::{self, SessionBinding};
use crate::middleware::auth::AdminClaims;
use crate::repositories::admin as repo;
use crate::services::{PasswordPolicy, PasswordPolicyCache};
use chrono::{DateTime, Utc};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::redis::RedisClient;
use sqlx::PgPool;
use tracing::{error, info, warn};

use super::auth_flow::{generate_admin_jwt, resolve_access_token_expiration_secs};

/// 拒绝刷新的原因；`reason()` 即审计 `failure_reason`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefreshDenial {
    UserNotFound,
    AccountDisabled,
    AccountLocked,
    PasswordResetRequired,
    PasswordExpired,
    /// 令牌签发后密码被修改（本人改密或管理员重置）。
    CredentialsChanged,
    /// 令牌签发后 MFA 状态发生变化（新绑定/重绑或再次完成验证）。
    MfaStateChanged,
}

impl RefreshDenial {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::UserNotFound => "user_not_found",
            Self::AccountDisabled => "account_disabled",
            Self::AccountLocked => "account_locked",
            Self::PasswordResetRequired => "password_reset_required",
            Self::PasswordExpired => "password_expired",
            Self::CredentialsChanged => "credentials_changed",
            Self::MfaStateChanged => "mfa_state_changed",
        }
    }
}

/// 复核刷新会话所需的上下文（纯数据，便于不依赖数据库的单元测试）。
pub(crate) struct RefreshContext<'a> {
    pub password_policy: &'a PasswordPolicy,
    /// 刷新令牌签发时间（秒级 Unix 时间戳）。
    pub issued_at: i64,
    pub now: DateTime<Utc>,
}

/// 按账号当前状态判定刷新令牌是否仍可使用。检查顺序与登录一致：
/// 账号存在 → 启用 → 未锁定 → 无需改密 → 密码未过期，然后是刷新特有的
/// "令牌签发后凭据/MFA 是否变化"。
pub(crate) fn evaluate_refresh_session<'u>(
    user: Option<&'u repo::AdminSessionStateRow>,
    ctx: &RefreshContext<'_>,
) -> Result<&'u repo::AdminSessionStateRow, RefreshDenial> {
    let Some(user) = user else {
        return Err(RefreshDenial::UserNotFound);
    };
    if !user.is_active {
        return Err(RefreshDenial::AccountDisabled);
    }
    if user.locked_until.is_some_and(|until| until > ctx.now) {
        return Err(RefreshDenial::AccountLocked);
    }
    if user.force_password_reset {
        return Err(RefreshDenial::PasswordResetRequired);
    }
    let (expired, _, _) = ctx
        .password_policy
        .check_password_expiry(Some(user.password_changed_at));
    if expired {
        return Err(RefreshDenial::PasswordExpired);
    }
    // 秒级比较：令牌 iat 截断到秒，同一秒内签发视为未变化，避免改密后立即
    // 登录得到的令牌被误判。
    if user.password_changed_at.timestamp() > ctx.issued_at {
        return Err(RefreshDenial::CredentialsChanged);
    }
    // 已绑定 MFA 的会话可以刷新；仅当签发后 MFA 状态变化（verified_at > iat）才拒绝。
    // 典型场景：无 MFA 时拿到刷新令牌后又自助绑定，或管理员重置后用户重新绑定。
    let mfa_touched_after_issue = user
        .mfa_changed_at
        .is_some_and(|at| at.timestamp() > ctx.issued_at);
    if mfa_touched_after_issue {
        return Err(RefreshDenial::MfaStateChanged);
    }
    Ok(user)
}

pub struct AdminRefreshParams<'a> {
    pub db: &'a PgPool,
    pub redis: &'a RedisClient,
    pub claims: &'a AdminClaims,
    pub jwt_secret: &'a str,
    pub jwt_expiration_secs_fallback: u64,
    pub password_policy_cache: &'a PasswordPolicyCache,
}

/// 用刷新令牌换发访问令牌。
///
/// 新访问令牌中的用户名与角色取自数据库当前值（而不是旧令牌），
/// 因此管理员调整角色后，下一次刷新即按新角色授权。
///
/// # Errors
///
/// - 令牌类型不是 refresh：400；
/// - 令牌已吊销或账号状态不允许继续会话：401，审计记录具体原因；
/// - 吊销状态无法确认（Redis 故障）：503；读取账号失败：500（均 fail-closed）。
pub async fn admin_refresh_token(
    params: AdminRefreshParams<'_>,
) -> Result<serde_json::Value, ApiError> {
    let claims = params.claims;
    match &claims.token_type {
        Some(tt) if tt == "refresh" => {}
        _ => {
            return Err(ApiError::validation(
                "无效的令牌类型，需要 refresh token".to_string(),
            ));
        }
    }
    revocation_check(params.redis, claims).await?;
    let Ok(user_id) = uuid::Uuid::parse_str(&claims.user_id) else {
        audit::set_failure_reason("invalid_token");
        return Err(ApiError::Unauthorized);
    };
    let user = repo::get_admin_session_state(params.db, user_id)
        .await
        .map_err(|e| {
            error!("Database error during token refresh: {}", e);
            audit::set_failure_reason("db_error");
            ApiError::internal("刷新令牌失败，请稍后重试".to_string())
        })?;
    let password_policy = params.password_policy_cache.get().await;
    let ctx = RefreshContext {
        password_policy: &password_policy,
        issued_at: claims.iat,
        now: Utc::now(),
    };
    let user = match evaluate_refresh_session(user.as_ref(), &ctx) {
        Ok(user) => user,
        Err(denial) => {
            warn!(
                "Token refresh denied for user {}: {}",
                claims.sub,
                denial.reason()
            );
            audit::set_failure_reason(denial.reason());
            audit::mark_security_event();
            crate::services::mfa::step_up::close_session(params.redis, &claims.sid).await;
            return Err(ApiError::Unauthorized);
        }
    };
    if user.role != claims.role || user.username != claims.sub {
        audit::add_detail(
            "refresh_identity_updated",
            serde_json::json!({ "role_changed": user.role != claims.role,
                                "username_changed": user.username != claims.sub }),
        );
    }
    let jwt_expiration_secs =
        resolve_access_token_expiration_secs(params.db, params.jwt_expiration_secs_fallback).await;
    let Some(sxp) = claims.sxp else {
        audit::set_failure_reason("legacy_token");
        return Err(ApiError::Unauthorized);
    };
    if uuid::Uuid::parse_str(&claims.sid).is_err() {
        audit::set_failure_reason("invalid_token");
        return Err(ApiError::Unauthorized);
    }
    let binding = SessionBinding {
        sid: claims.sid.clone(),
        sxp: Some(sxp),
    };
    let issued_at = Utc::now().timestamp();
    let new_token = generate_admin_jwt(
        &user.username,
        &claims.user_id,
        &user.role,
        &binding,
        params.jwt_secret,
        i64::saturating_from(jwt_expiration_secs),
        issued_at,
    )
    .map_err(|e| {
        error!("Failed to generate access JWT during refresh: {}", e);
        ApiError::internal("生成认证令牌失败".to_string())
    })?;
    info!(
        "Token refreshed for user: {} (role: {})",
        user.username, user.role
    );
    let expires_in = new_token.exp.saturating_sub(new_token.iat).max(1);
    Ok(serde_json::json!({ "token" : new_token.token, "expires_in" : expires_in }))
}

/// 刷新前再次确认刷新令牌未被吊销（中间件已查过一次，这里防止绕过中间件的调用方）。
/// 与中间件一致采用 fail-closed：Redis 故障返回 503 并记录 `token_check_failed`。
async fn revocation_check(redis: &RedisClient, claims: &AdminClaims) -> Result<(), ApiError> {
    match admin_token::ensure_not_revoked(redis, claims).await {
        Ok(()) => Ok(()),
        Err(rejection) => {
            audit::set_failure_reason(rejection.reason());
            if rejection == admin_token::TokenRejection::CheckUnavailable {
                Err(ApiError::Core(mazbri_core::CoreError::ServiceUnavailable {
                    service: "token revocation store".to_string(),
                }))
            } else {
                Err(ApiError::Unauthorized)
            }
        }
    }
}

#[cfg(test)]
mod tests;
