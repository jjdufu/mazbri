//! 敏感操作的 MFA 提权守卫。
//!
//! 普通敏感操作要求当前会话已有未过期的提权窗口。交出长期秘密、解除强制、
//! 拆掉或替换认证因子的操作改为一次性 fresh 授权，并且必须在入参与目标状态
//! 校验通过之后调用，不能做成提取器。

use crate::handlers::AppState;
use crate::middleware::auth::{AdminClaims, extract_user_id};
use crate::services::mfa::step_up::{self, FreshPurpose, WindowMiss};
use axum::{
    Json,
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use chrono::Utc;

/// 未启用 MFA 时前端引导用户前往的自助绑定页面。
pub const MFA_SETUP_PATH: &str = "/account/security";

/// 只证明当前会话窗口有效，不消费任何秘密。
#[derive(Debug, Clone, Copy)]
pub struct StepUpVerified;

/// 账号本身是否还能继续敏感操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountBlock {
    None,
    Inactive,
    LoginLocked,
    ForceReset,
    PasswordExpired,
}

/// MFA 绑定是否满足提权前提。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MfaReadiness {
    Ready,
    NotEnabled,
    Locked { retry_after_secs: i64 },
}

/// 守卫看到的账号快照。时间相关状态由调用方按同一 `now` 算好，便于单测。
#[derive(Debug, Clone)]
pub struct StepUpSubjectView {
    pub account: AccountBlock,
    pub role: String,
    pub mfa: MfaReadiness,
    pub auth_epoch: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectDenial {
    Unauthorized,
    SessionStale,
    NotEnabled,
    Locked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowReason {
    None,
    Expired,
    Invalidated,
}

impl WindowReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Expired => "expired",
            Self::Invalidated => "invalidated",
        }
    }
}

/// 按账号快照判断能否进入窗口或 fresh 检查。
///
/// # Errors
///
/// 会话无效、角色变化、未绑定或 MFA 锁定时返回对应拒绝原因。
pub fn evaluate_subject(
    subject: &StepUpSubjectView,
    claims_role: &str,
) -> Result<(), SubjectDenial> {
    if subject.account != AccountBlock::None {
        return Err(SubjectDenial::Unauthorized);
    }
    if subject.role != claims_role {
        return Err(SubjectDenial::SessionStale);
    }
    match subject.mfa {
        MfaReadiness::Ready => Ok(()),
        MfaReadiness::NotEnabled => Err(SubjectDenial::NotEnabled),
        MfaReadiness::Locked { .. } => Err(SubjectDenial::Locked),
    }
}

fn json_error(status: StatusCode, code: &str, message: &str, body: serde_json::Value) -> Response {
    crate::audit::set_failure_reason(audit_reason(code));
    crate::audit::mark_security_event();
    let mut response = (status, Json(body)).into_response();
    crate::audit::AuditErrorInfo::attach(&mut response, code, message);
    response
}

fn audit_reason(code: &str) -> &'static str {
    match code {
        "SESSION_STALE" => "session_stale",
        "MFA_NOT_ENABLED" => "mfa_not_enabled",
        "MFA_LOCKED" => "mfa_locked",
        "MFA_REQUIRED" => "mfa_required",
        "MFA_FRESH_REQUIRED" => "mfa_fresh_required",
        _ => "mfa_check_unavailable",
    }
}

fn unavailable() -> Response {
    let message = mazbri_i18n::mfa::check_unavailable();
    json_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "MFA_CHECK_UNAVAILABLE",
        &message,
        serde_json::json!({
            "success": false,
            "code": "MFA_CHECK_UNAVAILABLE",
            "error": message,
        }),
    )
}

fn not_enabled() -> Response {
    let message = "此操作需要 MFA 二次验证，请先在「账号安全」页面绑定并启用 MFA";
    json_error(
        StatusCode::FORBIDDEN,
        "MFA_NOT_ENABLED",
        message,
        serde_json::json!({
            "success": false,
            "code": "MFA_NOT_ENABLED",
            "error": message,
            "setup_path": MFA_SETUP_PATH,
        }),
    )
}

fn locked(retry_after_secs: i64) -> Response {
    let message = "MFA 已锁定，请稍后再试";
    json_error(
        StatusCode::FORBIDDEN,
        "MFA_LOCKED",
        message,
        serde_json::json!({
            "success": false,
            "code": "MFA_LOCKED",
            "retry_after_secs": retry_after_secs.max(0),
            "error": message,
        }),
    )
}

fn step_up_required(reason: WindowReason) -> Response {
    let message = mazbri_i18n::mfa::step_up_required();
    if reason == WindowReason::Invalidated {
        crate::audit::set_failure_reason("mfa_step_up_invalidated");
        crate::audit::mark_security_event();
        let mut response = (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "success": false,
                "code": "MFA_REQUIRED",
                "reason": reason.as_str(),
                "error": message,
            })),
        )
            .into_response();
        crate::audit::AuditErrorInfo::attach(&mut response, "MFA_REQUIRED", &message);
        return response;
    }
    json_error(
        StatusCode::FORBIDDEN,
        "MFA_REQUIRED",
        &message,
        serde_json::json!({
            "success": false,
            "code": "MFA_REQUIRED",
            "reason": reason.as_str(),
            "error": message,
        }),
    )
}

fn fresh_required(purpose: FreshPurpose) -> Response {
    let message = mazbri_i18n::mfa::fresh_required();
    json_error(
        StatusCode::FORBIDDEN,
        "MFA_FRESH_REQUIRED",
        &message,
        serde_json::json!({
            "success": false,
            "code": "MFA_FRESH_REQUIRED",
            "purpose": purpose.as_str(),
            "error": message,
        }),
    )
}

fn unauthorized(stale: bool) -> Response {
    if stale {
        crate::audit::set_failure_reason("session_stale");
        crate::audit::mark_security_event();
        let message = "会话角色已变化，请刷新后重试";
        let mut response = (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "success": false,
                "code": "SESSION_STALE",
                "error": message,
            })),
        )
            .into_response();
        crate::audit::AuditErrorInfo::attach(&mut response, "SESSION_STALE", message);
        return response;
    }
    crate::error::ApiError::Unauthorized.into_response()
}

struct CheckedSubject {
    user_id: uuid::Uuid,
    epoch: i64,
}

async fn load_checked_subject(
    state: &AppState,
    claims: &AdminClaims,
) -> Result<CheckedSubject, Box<Response>> {
    if uuid::Uuid::parse_str(&claims.sid).is_err() {
        return Err(Box::new(unauthorized(false)));
    }
    let user_id = extract_user_id(claims).map_err(|error| Box::new(error.into_response()))?;
    let row = crate::repositories::admin::get_step_up_subject(&state.admin_db, user_id)
        .await
        .map_err(|_| Box::new(unavailable()))?
        .ok_or_else(|| Box::new(unauthorized(false)))?;
    let now = Utc::now();
    let policy = state.password_policy_cache.get().await;
    let (password_expired, _, _) = policy.check_password_expiry(Some(row.password_changed_at));
    let retry_after_secs = row
        .mfa_locked_until
        .map_or(0, |until| step_up::retry_after(until, now));
    let account = if !row.is_active() {
        AccountBlock::Inactive
    } else if row.locked_until.is_some_and(|until| until > now) {
        AccountBlock::LoginLocked
    } else if row.force_password_reset() {
        AccountBlock::ForceReset
    } else if password_expired {
        AccountBlock::PasswordExpired
    } else {
        AccountBlock::None
    };
    let mfa = if !row.mfa_enabled() || !row.has_secret() {
        MfaReadiness::NotEnabled
    } else if crate::mfa::MfaService::is_locked(row.mfa_locked_until, now) {
        MfaReadiness::Locked { retry_after_secs }
    } else {
        MfaReadiness::Ready
    };
    let view = StepUpSubjectView {
        account,
        role: row.role,
        mfa,
        auth_epoch: row.auth_epoch,
    };
    match evaluate_subject(&view, &claims.role) {
        Ok(()) => Ok(CheckedSubject {
            user_id,
            epoch: view.auth_epoch,
        }),
        Err(SubjectDenial::Unauthorized) => Err(Box::new(unauthorized(false))),
        Err(SubjectDenial::SessionStale) => Err(Box::new(unauthorized(true))),
        Err(SubjectDenial::NotEnabled) => Err(Box::new(not_enabled())),
        Err(SubjectDenial::Locked) => Err(Box::new(locked(retry_after_secs))),
    }
}

/// 要求当前会话的提权窗口有效。
///
/// # Errors
///
/// 会话无效返回 401；未绑定、锁定或没有窗口返回 403；依赖故障返回 503。
pub async fn require_step_up(
    state: &AppState,
    claims: &AdminClaims,
) -> Result<StepUpVerified, Box<Response>> {
    let checked = load_checked_subject(state, claims).await?;
    let window = step_up::load_window(&state.redis, &claims.sid)
        .await
        .map_err(|_| Box::new(unavailable()))?
        .ok_or_else(|| Box::new(step_up_required(WindowReason::None)))?;
    let now = Utc::now().timestamp();
    match step_up::classify_window(&window, &checked.user_id.to_string(), checked.epoch, now) {
        Ok(()) => {
            crate::audit::add_detail("mfa", serde_json::json!("step_up"));
            crate::audit::add_detail("step_up_opened_at", serde_json::json!(window.opened_at));
            crate::audit::add_detail("step_up_method", serde_json::json!(window.method.as_str()));
            Ok(StepUpVerified)
        }
        Err(WindowMiss::Expired) => Err(Box::new(step_up_required(WindowReason::Expired))),
        Err(WindowMiss::Invalidated) => {
            if let Err(error) = step_up::delete_if_match(
                &state.redis,
                &format!("admin:mfa_step_up:{}", claims.sid),
                &window.uid,
                window.epoch,
                "",
            )
            .await
            {
                tracing::warn!("failed to drop invalidated step-up window: {error}");
            }
            Err(Box::new(step_up_required(WindowReason::Invalidated)))
        }
    }
}

/// 消费一次性 fresh 授权。不要求窗口同时有效。
///
/// # Errors
///
/// 没有匹配的授权返回 403 `MFA_FRESH_REQUIRED`。
pub async fn require_fresh_mfa(
    state: &AppState,
    claims: &AdminClaims,
    purpose: FreshPurpose,
) -> Result<(), Box<Response>> {
    let checked = load_checked_subject(state, claims).await?;
    let consumed = step_up::consume_fresh(
        &state.redis,
        &claims.sid,
        &checked.user_id.to_string(),
        checked.epoch,
        purpose,
    )
    .await
    .map_err(|_| Box::new(unavailable()))?;
    if !consumed {
        return Err(Box::new(fresh_required(purpose)));
    }
    crate::audit::add_detail("mfa", serde_json::json!("fresh"));
    crate::audit::add_detail("purpose", serde_json::json!(purpose.as_str()));
    Ok(())
}

impl FromRequestParts<AppState> for StepUpVerified {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = parts
            .extensions
            .get::<AdminClaims>()
            .cloned()
            .ok_or_else(|| crate::error::ApiError::Unauthorized.into_response())?;
        require_step_up(state, &claims)
            .await
            .map_err(|response| *response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject() -> StepUpSubjectView {
        StepUpSubjectView {
            account: AccountBlock::None,
            role: "R_FINANCE".to_string(),
            mfa: MfaReadiness::Ready,
            auth_epoch: 1,
        }
    }

    #[test]
    fn active_bound_subject_passes() {
        assert!(evaluate_subject(&subject(), "R_FINANCE").is_ok());
    }

    #[test]
    fn inactive_locked_reset_and_expired_password_are_unauthorized() {
        for account in [
            AccountBlock::Inactive,
            AccountBlock::LoginLocked,
            AccountBlock::ForceReset,
            AccountBlock::PasswordExpired,
        ] {
            let mut view = subject();
            view.account = account;
            assert_eq!(
                evaluate_subject(&view, "R_FINANCE"),
                Err(SubjectDenial::Unauthorized)
            );
        }
    }

    #[test]
    fn role_mismatch_is_session_stale() {
        assert_eq!(
            evaluate_subject(&subject(), "R_ADMIN"),
            Err(SubjectDenial::SessionStale)
        );
    }

    #[test]
    fn unbound_is_not_enabled_and_mfa_lock_is_locked() {
        let mut unbound = subject();
        unbound.mfa = MfaReadiness::NotEnabled;
        assert_eq!(
            evaluate_subject(&unbound, "R_FINANCE"),
            Err(SubjectDenial::NotEnabled)
        );
        let mut locked = subject();
        locked.mfa = MfaReadiness::Locked {
            retry_after_secs: 90,
        };
        assert_eq!(
            evaluate_subject(&locked, "R_FINANCE"),
            Err(SubjectDenial::Locked)
        );
    }

    fn handler_source<'a>(source: &'a str, name: &str) -> &'a str {
        let start = source
            .find(&format!("pub async fn {name}("))
            .unwrap_or_else(|| panic!("handler {name} not found"));
        let rest = &source[start..];
        let end = rest.find("\n}\n").unwrap_or(rest.len());
        &rest[..end]
    }

    #[test]
    fn gated_handlers_are_wired_to_the_guard() {
        let crud = include_str!("admin_management/crud.rs");
        let password = include_str!("admin_management/password.rs");
        let status = include_str!("admin_management/status.rs");
        let admin_mfa = include_str!("admin_management/mfa.rs");
        let cc_config = include_str!("config_center/config.rs");
        let cc_history = include_str!("config_center/history.rs");
        let cc_import = include_str!("config_center/import_export.rs");
        let settings = include_str!("admin/settings.rs");
        let password_policy = include_str!("password_policy_handlers.rs");
        let mfa_settings = include_str!("mfa_settings_handlers.rs");
        let tenants = include_str!("tenant_handlers.rs");
        let billing = include_str!("billing_handlers.rs");
        let account = include_str!("product/account.rs");
        let ratelimit = include_str!("ratelimit_command_handlers.rs");
        let upstream = include_str!("upstream_connector_handlers.rs");
        let crypto = include_str!("tenant_crypto_handlers.rs");
        let api_keys = include_str!("api_key_handlers.rs");
        let certs = include_str!("../proxy/cert/certificates.rs");

        let window: &[(&str, &str)] = &[
            (crud, "create_admin"),
            (crud, "delete_admin"),
            (password, "force_password_reset"),
            (status, "set_admin_status"),
            (cc_config, "create_config"),
            (cc_config, "update_config"),
            (cc_config, "delete_config"),
            (cc_history, "rollback"),
            (cc_import, "import_configs"),
            (cc_import, "export_configs"),
            (settings, "update_system_settings"),
            (password_policy, "update_password_policy"),
            (tenants, "delete_tenant"),
            (billing, "delete_reconciliation_discrepancy"),
            (billing, "delete_bill"),
            (ratelimit, "create_control_command"),
            (upstream, "rollback"),
            (api_keys, "revoke_api_key"),
            (api_keys, "update_api_key"),
            (account, "recharge_tenant_account"),
            (account, "refund_tenant_account"),
        ];
        for (source, name) in window {
            let body = handler_source(source, name);
            assert!(
                body.contains("StepUpVerified") || body.contains("require_step_up("),
                "{name} must require a step-up window"
            );
        }

        let fresh: &[(&str, &str, &str)] = &[
            (password, "reset_admin_password", "AdminResetPassword"),
            (admin_mfa, "reset_admin_mfa", "AdminResetMfa"),
            (mfa_settings, "update_mfa_settings", "MfaSettingsUpdate"),
            (crypto, "rotate_crypto_key", "CryptoKeyReveal"),
            (api_keys, "create_api_key", "ApiKeyCreate"),
            (certs, "issue_certificate_proxy", "CertificateIssue"),
        ];
        for (source, name, purpose) in fresh {
            let body = handler_source(source, name);
            assert!(
                body.contains("require_fresh_mfa(") && body.contains(purpose),
                "{name} must consume fresh {purpose}"
            );
        }
        let update = handler_source(crud, "update_admin");
        assert!(update.contains("MfaRequirementLift") && update.contains("require_step_up("));
        let publish = handler_source(upstream, "publish_draft");
        assert!(publish.contains("force_publish") && publish.contains("require_step_up("));
        let export = handler_source(crypto, "export_crypto_data");
        assert!(export.contains("is_migration_export") && export.contains("require_fresh_mfa("));
        assert!(certs.contains("CertificateRenew"));
        assert!(certs.contains("CertificateFull"));
        assert!(certs.contains("fresh_renew") || certs.contains("CertificateRenew"));
    }
}
