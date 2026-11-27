use crate::audit::{self, AuditAction, AuditService, user_audit};
use crate::error::ApiError;
use crate::handlers::types::{AdminLoginRequest, AdminLogoutRequest, AdminUser};
use crate::middleware::admin_token::{self, SessionBinding, TokenKind, TokenSubject};
use crate::middleware::auth::{AdminClaims, ROLE_SUPER_ADMIN, revoke_jti};
use crate::repositories::admin as repo;
use crate::services::{MfaSettings, MfaSettingsCache, PasswordPolicy, PasswordPolicyCache};
use chrono::Utc;
use mazbri_core::num::SaturatingFrom;
use mazbri_core::panic_policy::spawn_blocking_recoverable;
use mazbri_core::redis::RedisClient;
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

const ACCESS_TOKEN_MIN_SECS: u64 = 60;
const ACCESS_TOKEN_MAX_SECS: u64 = 3600;
const SESSION_TIMEOUT_MIN_SECS: u64 = 300;
const SESSION_TIMEOUT_MAX_SECS: u64 = 604_800;
const LOGIN_IP_WINDOW_SECS: u64 = 900;
const LOGIN_IP_MAX_FAILURES: i64 = 5;
const UNIFIED_LOGIN_AUTH_ERR_MSG: &str = "用户名或密码无效";

const INCR_WITH_TTL_LUA: &str = r"
local n = redis.call('INCR', KEYS[1])
if n == 1 then
  redis.call('EXPIRE', KEYS[1], tonumber(ARGV[1]))
end
return n
";

/// 固定短效访问令牌有效期（60s ~ 3600s），不受运行时 `session_timeout` 覆盖。
#[must_use]
pub fn clamp_access_token_expiration_secs(configured_secs: u64) -> u64 {
    configured_secs.clamp(ACCESS_TOKEN_MIN_SECS, ACCESS_TOKEN_MAX_SECS)
}

/// 解析正式访问令牌有效期（秒）。仅受启动配置控制并钳制在 `[60, 3600]`。
pub async fn resolve_access_token_expiration_secs(_db: &PgPool, fallback_secs: u64) -> u64 {
    clamp_access_token_expiration_secs(fallback_secs)
}

/// 解析登录会话绝对存活期（`sxp` 与 Refresh Cookie `Max-Age`，秒），优先读取 `system_settings.session_timeout`（ms）。
pub async fn resolve_session_timeout_secs(db: &PgPool, fallback_secs: u64) -> u64 {
    match repo::get_system_setting_value(db, "session_timeout").await {
        Ok(Some(value)) => {
            let ms = value
                .as_i64()
                .or_else(|| value.as_u64().map(i64::saturating_from))
                .or_else(|| value.as_str().and_then(|s| s.parse().ok()));
            match ms {
                Some(ms) if ms > 0 => (u64::saturating_from(ms) / 1000)
                    .clamp(SESSION_TIMEOUT_MIN_SECS, SESSION_TIMEOUT_MAX_SECS),
                _ => fallback_secs.clamp(SESSION_TIMEOUT_MIN_SECS, SESSION_TIMEOUT_MAX_SECS),
            }
        }
        _ => fallback_secs.clamp(SESSION_TIMEOUT_MIN_SECS, SESSION_TIMEOUT_MAX_SECS),
    }
}

fn login_fail_ip_key(ip: &str) -> String {
    format!("login_fail:ip:{ip}")
}

fn login_fail_user_ip_key(username_norm: &str, ip: &str) -> String {
    format!("login_fail:user_ip:{username_norm}:{ip}")
}

async fn read_counter(redis: &RedisClient, key: &str) -> i64 {
    let Ok(Some(bytes)) = redis.get_bytes(key).await else {
        return 0;
    };
    std::str::from_utf8(&bytes)
        .ok()
        .and_then(|raw| raw.trim().parse::<i64>().ok())
        .unwrap_or(0)
}

async fn check_login_ip_rate_limit(
    redis: &RedisClient,
    username_norm: &str,
    client_ip: Option<&str>,
) -> Result<(), ApiError> {
    let Some(ip) = client_ip.map(str::trim).filter(|ip| !ip.is_empty()) else {
        return Ok(());
    };
    let ip_key = login_fail_ip_key(ip);
    let user_ip_key = login_fail_user_ip_key(username_norm, ip);
    let ip_count = read_counter(redis, &ip_key).await;
    let user_ip_count = read_counter(redis, &user_ip_key).await;
    if ip_count >= LOGIN_IP_MAX_FAILURES || user_ip_count >= LOGIN_IP_MAX_FAILURES {
        let ttl_key = if user_ip_count >= LOGIN_IP_MAX_FAILURES {
            &user_ip_key
        } else {
            &ip_key
        };
        let retry_after = redis
            .ttl(ttl_key)
            .await
            .ok()
            .and_then(|secs| (secs > 0).then(|| u64::saturating_from(secs)))
            .unwrap_or(LOGIN_IP_WINDOW_SECS);
        audit::set_failure_reason("rate_limited");
        audit::mark_security_event();
        return Err(ApiError::rate_limited(retry_after));
    }
    Ok(())
}

async fn record_login_ip_failure(
    redis: &RedisClient,
    username_norm: &str,
    client_ip: Option<&str>,
) {
    let Some(ip) = client_ip.map(str::trim).filter(|ip| !ip.is_empty()) else {
        return;
    };
    let ttl_arg = LOGIN_IP_WINDOW_SECS.to_string();
    for key in [
        login_fail_ip_key(ip),
        login_fail_user_ip_key(username_norm, ip),
    ] {
        if let Err(error) = redis
            .eval_i64(INCR_WITH_TTL_LUA, &[key], std::slice::from_ref(&ttl_arg))
            .await
        {
            warn!("Failed to increment Redis login failure counter: {error}");
        }
    }
}

async fn clear_user_ip_login_failures(
    redis: &RedisClient,
    username_norm: &str,
    client_ip: Option<&str>,
) {
    let Some(ip) = client_ip.map(str::trim).filter(|ip| !ip.is_empty()) else {
        return;
    };
    if let Err(error) = redis.del(&login_fail_user_ip_key(username_norm, ip)).await {
        warn!("Failed to clear user_ip login failure counter: {error}");
    }
}

/// 签发正式访问令牌（`token_type = "access"`）。
#[expect(clippy::too_many_arguments, reason = "JWT issuance binds identity, epoch, session, and TTL")]
pub(crate) fn generate_admin_jwt(
    username: &str,
    user_id: &str,
    role: &str,
    auth_epoch: i64,
    binding: &SessionBinding,
    jwt_secret: &str,
    expiration_secs: i64,
    issued_at: i64,
) -> Result<admin_token::IssuedToken, String> {
    let sxp = binding
        .sxp
        .ok_or_else(|| "access token requires sxp".to_string())?;
    let ttl = admin_token::clamp_access_ttl(expiration_secs, sxp, issued_at)?;
    admin_token::issue_at(
        TokenKind::Access,
        TokenSubject {
            username,
            user_id,
            role,
            auth_epoch,
            binding,
        },
        jwt_secret,
        ttl,
        issued_at,
    )
}
pub(crate) fn issue_refresh_token(
    username: &str,
    user_id: &str,
    role: &str,
    auth_epoch: i64,
    binding: &SessionBinding,
    jwt_secret: &str,
    issued_at: i64,
) -> Result<admin_token::IssuedToken, String> {
    let sxp = binding
        .sxp
        .ok_or_else(|| "refresh token requires sxp".to_string())?;
    let ttl = sxp.saturating_sub(issued_at);
    if ttl < 1 {
        return Err("session absolute expiry is not in the future".to_string());
    }
    admin_token::issue_at(
        TokenKind::Refresh,
        TokenSubject {
            username,
            user_id,
            role,
            auth_epoch,
            binding,
        },
        jwt_secret,
        ttl,
        issued_at,
    )
}
/// 签发登录中间阶段的一次性临时令牌（MFA 验证 / MFA 设置 / 强制改密）。
pub(crate) fn issue_stage_token(
    kind: TokenKind,
    username: &str,
    user_id: uuid::Uuid,
    role: &str,
    auth_epoch: i64,
    binding: &SessionBinding,
    jwt_secret: &str,
) -> Result<String, ApiError> {
    let user_id = user_id.to_string();
    admin_token::issue_temporary(
        kind,
        TokenSubject {
            username,
            user_id: &user_id,
            role,
            auth_epoch,
            binding,
        },
        jwt_secret,
    )
    .map_err(|e| {
        error!("Failed to generate {} temp JWT: {}", kind.as_str(), e);
        ApiError::internal("生成认证令牌失败".to_string())
    })
}
fn resolve_password_hash(user_row: &repo::AdminUserFullRow) -> Result<String, ApiError> {
    match user_row.password_hash.as_deref() {
        Some(hash) if !hash.is_empty() => Ok(hash.to_string()),
        _ => {
            error!(
                "No password_hash in database for user {}. Password verification failed. \
                 / 用户 {} 在数据库中没有 password_hash，密码验证失败。",
                user_row.username, user_row.username
            );
            audit::set_failure_reason("password_hash_missing");
            Err(ApiError::validation("密码验证失败".to_string()))
        }
    }
}
async fn handle_failed_login(
    db: &PgPool,
    redis: &RedisClient,
    user_row: &repo::AdminUserFullRow,
    policy: &PasswordPolicy,
    username_norm: &str,
    client_ip: Option<&str>,
) -> Result<serde_json::Value, ApiError> {
    let user_id = user_row.id;
    let username = user_row.username.as_str();
    record_login_ip_failure(redis, username_norm, client_ip).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    if user_row.role == ROLE_SUPER_ADMIN {
        audit::set_failure_reason("invalid_password");
        return Err(ApiError::auth_failed(UNIFIED_LOGIN_AUTH_ERR_MSG));
    }
    let updated = match repo::record_login_failure_outcome(
        db,
        user_id,
        policy.max_login_attempts,
        i64::from(policy.lockout_duration_minutes),
    )
    .await
    {
        Ok(Some(updated)) => updated,
        Ok(None) => {
            audit::set_failure_reason("invalid_password");
            return Err(ApiError::auth_failed(UNIFIED_LOGIN_AUTH_ERR_MSG));
        }
        Err(e) => {
            error!(
                "Failed to update login failure count for user {}: {}. Lockout bypass possible!",
                username, e
            );
            audit::set_failure_reason("db_error");
            return Err(ApiError::internal("登录验证失败，请稍后重试".to_string()));
        }
    };
    if updated
        .locked_until
        .is_some_and(|locked_until| locked_until > Utc::now())
    {
        warn!(
            "User {} locked after {} failed login attempts",
            username, updated.login_attempts
        );
        audit::set_failure_reason("lock_triggered");
        audit::mark_security_event();
        return Err(ApiError::auth_failed(UNIFIED_LOGIN_AUTH_ERR_MSG));
    }
    audit::set_failure_reason("invalid_password");
    Err(ApiError::auth_failed(UNIFIED_LOGIN_AUTH_ERR_MSG))
}
/// 密码校验通过后判断是否需要进入 MFA 阶段；需要时返回带一次性临时令牌的阶段响应。
///
/// # Errors
///
/// MFA 已锁定或令牌签发失败时返回错误。
pub(crate) fn determine_mfa(
    user_row: &repo::AdminUserFullRow,
    mfa_settings: &MfaSettings,
    binding: &SessionBinding,
    jwt_secret: &str,
) -> Result<Option<serde_json::Value>, ApiError> {
    let username = user_row.username.as_str();
    let role = user_row.role.as_str();
    let user_id = user_row.id;
    let auth_epoch = user_row.auth_epoch;
    let totp_digits = mfa_settings.totp_digits;
    if user_row.mfa.mfa_enabled {
        if let Some(mfa_lock_until) = user_row.mfa_locked_until
            && mfa_lock_until > Utc::now()
        {
            warn!("Login blocked: MFA locked for user: {}", username);
            audit::set_failure_reason("mfa_locked");
            return Err(ApiError::validation(
                "由于失败次数过多，MFA 已锁定。请稍后再试。".to_string(),
            ));
        }
        let temp_token = issue_stage_token(
            TokenKind::MfaPending,
            username,
            user_id,
            role,
            auth_epoch,
            binding,
            jwt_secret,
        )?;
        info!("Password verified, MFA required for user: {}", username);
        audit::add_detail("login_stage", serde_json::json!("mfa_required"));
        return Ok(Some(serde_json::json!(
            { "mfa_required" : true, "temp_token" : temp_token, "totp_digits"
            : totp_digits, "message" : "需要 MFA 验证" }
        )));
    }
    // 超管为该账号设置了 mfa_required 且尚未绑定时，进入强制设置阶段。
    if user_row.mfa.mfa_required {
        let temp_token = issue_stage_token(
            TokenKind::MfaSetup,
            username,
            user_id,
            role,
            auth_epoch,
            binding,
            jwt_secret,
        )?;
        info!(
            "Password verified, MFA setup required for user: {} (per-user mfa_required)",
            username
        );
        audit::add_detail("login_stage", serde_json::json!("mfa_setup_required"));
        return Ok(Some(serde_json::json!({
            "mfa_setup_required": true,
            "temp_token": temp_token,
            "message": "安全策略要求设置 MFA",
        })));
    }
    // 未绑定且未要求 MFA：直接登录；敏感功能仍按功能要求 MFA。
    Ok(None)
}
struct GenerateLoginResponseParams<'a> {
    db: &'a PgPool,
    username: &'a str,
    user_id: uuid::Uuid,
    role: &'a str,
    auth_epoch: i64,
    created_at: &'a str,
    binding: &'a SessionBinding,
    jwt_secret: &'a str,
    jwt_expiration_secs: u64,
    session_timeout_secs: u64,
    audit: &'a AuditService,
    client_ip: Option<String>,
}

async fn generate_login_response(
    params: GenerateLoginResponseParams<'_>,
) -> Result<serde_json::Value, ApiError> {
    let issued_at = Utc::now().timestamp();
    let token = generate_admin_jwt(
        params.username,
        &params.user_id.to_string(),
        params.role,
        params.auth_epoch,
        params.binding,
        params.jwt_secret,
        i64::saturating_from(params.jwt_expiration_secs),
        issued_at,
    )
    .map_err(|e| {
        error!("Failed to generate admin JWT: {}", e);
        ApiError::internal("生成认证令牌失败".to_string())
    })?;
    let refresh_token = issue_refresh_token(
        params.username,
        &params.user_id.to_string(),
        params.role,
        params.auth_epoch,
        params.binding,
        params.jwt_secret,
        issued_at,
    )
    .map_err(|e| {
        error!("Failed to generate refresh JWT: {}", e);
        ApiError::internal("生成刷新令牌失败".to_string())
    })?;
    let user = AdminUser {
        id: params.user_id.to_string(),
        username: params.username.to_string(),
        role: params.role.to_string(),
        created_at: params.created_at.to_string(),
    };
    if let Err(e) = repo::update_last_login(params.db, params.user_id).await {
        warn!("Failed to update last_login_at: {}", e);
    }
    info!(
        "Admin login successful: {} (role: {})",
        params.username, user.role
    );
    if let Err(e) = params
        .audit
        .log_critical(user_audit(
            params.user_id,
            AuditAction::UserLogin,
            Some(serde_json::json!({ "username" : params.username, "role" : user.role, })),
            params.client_ip,
        ))
        .await
    {
        error!(
            "Failed to write critical audit log for login success: {}",
            e
        );
    }
    let expires_in = u64::try_from(token.exp.saturating_sub(token.iat).max(1))
        .unwrap_or(params.jwt_expiration_secs);
    Ok(serde_json::json!(
        { "user" : user, "token" : token.token, "expires_in" : expires_in,
        "refresh_token" : refresh_token.token, "refresh_max_age" : params.session_timeout_secs }
    ))
}

/// 按用户名加载可登录账户（不存在/禁用/锁定时返回错误）。
async fn load_login_user(
    db: &PgPool,
    redis: &RedisClient,
    username: &str,
    username_norm: &str,
    client_ip: Option<&str>,
) -> Result<repo::AdminUserFullRow, ApiError> {
    let user_row = match repo::get_admin_user_by_username(db, username).await {
        Ok(row) => row,
        Err(e) => {
            error!("Database error during login: {}", e);
            audit::set_failure_reason("db_error");
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            return Err(ApiError::internal(
                "Database error during login".to_string(),
            ));
        }
    };
    let Some(user_row) = user_row else {
        record_login_ip_failure(redis, username_norm, client_ip).await;
        // 不存在的账号名可能是误输入的密码，落库前脱敏。
        audit::mask_attempted_username();
        audit::set_failure_reason("user_not_found");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        return Err(ApiError::auth_failed(UNIFIED_LOGIN_AUTH_ERR_MSG));
    };
    let username = &user_row.username;
    if !user_row.is_active {
        warn!("Login attempt for disabled user: {}", username);
        audit::set_failure_reason("account_disabled");
        return Err(ApiError::validation("账户已禁用".to_string()));
    }
    if user_row.role != ROLE_SUPER_ADMIN
        && let Some(lock_until) = user_row.locked_until
        && lock_until > Utc::now()
    {
        let remaining_seconds = (lock_until - Utc::now()).num_seconds();
        warn!(
            "Login attempt for locked user: {}, locked until: {}",
            username, lock_until
        );
        audit::set_failure_reason("account_locked");
        return Err(ApiError::validation(format!(
            "账户已被临时锁定。请在 {remaining_seconds} 秒后重试。"
        )));
    }
    Ok(user_row)
}

pub struct AdminLoginParams<'a> {
    pub db: &'a PgPool,
    pub redis: &'a RedisClient,
    pub jwt_secret: &'a str,
    pub jwt_expiration_secs: u64,
    pub refresh_token_expiration_secs: u64,
    pub password_policy_cache: &'a PasswordPolicyCache,
    pub mfa_settings_cache: &'a MfaSettingsCache,
    pub audit: &'a AuditService,
    pub req: &'a AdminLoginRequest,
    pub client_ip: Option<String>,
}

async fn verify_login_password(
    password: &str,
    admin_password_hash: String,
) -> Result<bool, ApiError> {
    let password_bytes = password.as_bytes().to_vec();
    spawn_blocking_recoverable(move || {
        mazbri_auth::constant_time_argon2_verify(&password_bytes, &admin_password_hash).is_ok()
    })
    .await
    .map_err(|e| ApiError::internal(format!("密码校验任务异常: {e}")))?
    .map_err(|e| ApiError::internal(format!("密码校验异常: {e}")))
}

fn log_login_stage_challenge(
    audit_svc: &AuditService,
    user_id: uuid::Uuid,
    username: &str,
    role: &str,
    action: AuditAction,
    stage: &str,
    client_ip: Option<String>,
) {
    audit_svc.log(user_audit(
        user_id,
        action,
        Some(serde_json::json!({
            "username": username,
            "role": role,
            "stage_info": stage,
        })),
        client_ip,
    ));
}

async fn check_post_password_stages(
    params: &AdminLoginParams<'_>,
    user_row: &repo::AdminUserFullRow,
    binding: &SessionBinding,
    client_ip: Option<String>,
) -> Result<Option<serde_json::Value>, ApiError> {
    let user_id = user_row.id;
    let username = user_row.username.as_str();
    let role = user_row.role.as_str();
    if user_row.force_password_reset {
        audit::add_detail("login_stage", serde_json::json!("password_reset_required"));
        log_login_stage_challenge(
            params.audit,
            user_id,
            username,
            role,
            AuditAction::UserLoginPasswordResetRequired,
            "password_reset_required",
            client_ip,
        );
        let temp_token = issue_stage_token(
            TokenKind::PasswordReset,
            username,
            user_id,
            role,
            user_row.auth_epoch,
            binding,
            params.jwt_secret,
        )?;
        info!("Password verified, password reset required for user: {username}");
        return Ok(Some(serde_json::json!(
            { "password_reset_required" : true, "username" : username, "role" : role, "temp_token" :
            temp_token, "message" : "需要重置密码。请修改您的密码。" }
        )));
    }
    let policy = params.password_policy_cache.get().await;
    let (is_expired, days_until_expiry, _should_warn) =
        policy.check_password_expiry(Some(user_row.password_changed_at));
    if is_expired {
        warn!(
            "Login blocked: password expired for user: {} (days until expiry: {})",
            username, days_until_expiry
        );
        audit::set_failure_reason("password_expired");
        return Err(ApiError::validation(
            "密码已过期。请联系管理员重置密码。".to_string(),
        ));
    }
    if (user_row.login_attempts > 0 || user_row.locked_until.is_some())
        && let Err(e) = repo::reset_login_attempts(params.db, user_id).await
    {
        error!("Failed to reset login failure count for user {username}: {e}");
    }
    let mfa_settings = params.mfa_settings_cache.get().await;
    if let Some(mfa_response) =
        determine_mfa(user_row, &mfa_settings, binding, params.jwt_secret)?
    {
        let next_step = if mfa_response
            .get("mfa_setup_required")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            "mfa_setup_required"
        } else {
            "mfa_required"
        };
        log_login_stage_challenge(
            params.audit,
            user_id,
            username,
            role,
            AuditAction::UserLoginMfaChallenge,
            next_step,
            client_ip,
        );
        return Ok(Some(mfa_response));
    }
    Ok(None)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn admin_login(params: AdminLoginParams<'_>) -> Result<serde_json::Value, ApiError> {
    let db = params.db;
    let redis = params.redis;
    let jwt_secret = params.jwt_secret;
    let jwt_expiration_secs = clamp_access_token_expiration_secs(params.jwt_expiration_secs);
    let session_timeout_secs =
        resolve_session_timeout_secs(db, params.refresh_token_expiration_secs).await;
    let req = params.req;
    let client_ip = params.client_ip.clone();
    let username_norm = req.username.trim().to_ascii_lowercase();
    audit::set_attempted_username(&req.username);
    check_login_ip_rate_limit(redis, &username_norm, client_ip.as_deref()).await?;
    let user_row =
        load_login_user(db, redis, &req.username, &username_norm, client_ip.as_deref()).await?;
    let user_id = user_row.id;
    let username = &user_row.username;
    let role = &user_row.role;
    let auth_epoch = user_row.auth_epoch;
    let admin_password_hash = resolve_password_hash(&user_row)?;
    if !verify_login_password(&req.password, admin_password_hash).await? {
        let policy = params.password_policy_cache.get().await;
        return handle_failed_login(
            db,
            redis,
            &user_row,
            &policy,
            &username_norm,
            client_ip.as_deref(),
        )
        .await;
    }
    clear_user_ip_login_failures(redis, &username_norm, client_ip.as_deref()).await;
    audit::set_actor(Some(user_id), Some(username), Some(role));
    let now = Utc::now().timestamp();
    let binding = SessionBinding::new_login(now, i64::saturating_from(session_timeout_secs));
    if let Some(stage_resp) =
        check_post_password_stages(&params, &user_row, &binding, client_ip.clone()).await?
    {
        return Ok(stage_resp);
    }
    let created_at = user_row.created_at.to_rfc3339();
    generate_login_response(GenerateLoginResponseParams {
        db,
        username,
        user_id,
        role,
        auth_epoch,
        created_at: &created_at,
        binding: &binding,
        jwt_secret,
        jwt_expiration_secs,
        session_timeout_secs,
        audit: params.audit,
        client_ip,
    })
    .await
}
/// Logout: revoke access jti (+ optional refresh) and audit (H-11).
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn admin_logout(
    claims: &AdminClaims,
    jwt_secret: &str,
    redis: &RedisClient,
    audit: &AuditService,
    req: &AdminLogoutRequest,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    let now = Utc::now().timestamp();
    if let Some(ref jti) = claims.jti {
        let ttl = (claims.exp - now).max(1);
        if let Err(e) = revoke_jti(redis, jti, ttl).await {
            error!("Failed to revoke access token jti: {}", e);
        }
    }
    if let Some(ref refresh_token) = req.refresh_token
        && let Ok(refresh_claims) = admin_token::decode_refresh(refresh_token, jwt_secret)
        && let Some(ref jti) = refresh_claims.jti
    {
        let ttl = (refresh_claims.exp - now).max(1);
        if let Err(e) = revoke_jti(redis, jti, ttl).await {
            error!("Failed to revoke refresh token jti: {}", e);
        }
    }
    crate::services::mfa::step_up::close_session(redis, &claims.sid).await;
    let user_id = Uuid::parse_str(&claims.user_id).unwrap_or(Uuid::nil());
    if let Err(e) = audit
        .log_critical(user_audit(
            user_id,
            AuditAction::UserLogout,
            Some(serde_json::json!({
                "username": claims.sub,
                "role": claims.role,
                "step_up_closed": true
            })),
            client_ip,
        ))
        .await
    {
        error!("Failed to write audit log for logout: {}", e);
    }
    info!("Admin logout: {} (role: {})", claims.sub, claims.role);
    Ok(serde_json::json!({ "message": "已退出登录" }))
}
