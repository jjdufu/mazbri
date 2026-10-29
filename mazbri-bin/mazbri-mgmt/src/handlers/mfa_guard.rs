//! 敏感操作的 MFA 提权守卫。
//!
//! 每类操作要求当前会话已有未过期的该类窗口。窗口在有效期内可重复使用。
//! 守卫必须在该操作原有的入参与目标状态校验通过之后调用，不能做成提取器。

use crate::handlers::AppState;
use crate::middleware::auth::{AdminClaims, extract_user_id};
use crate::services::mfa::step_up::{self, Category, WindowMiss};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;

/// 未启用 MFA 时前端引导用户前往的自助绑定页面。
pub const MFA_SETUP_PATH: &str = "/account/security";

/// 只证明当前会话的这一类窗口有效，不消费窗口。
#[derive(Debug, Clone, Copy)]
pub struct CategoryVerified(pub Category);

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

fn step_up_required(category: Category, reason: WindowReason) -> Response {
    let message = mazbri_i18n::mfa::step_up_required();
    let body = serde_json::json!({
        "success": false,
        "code": "MFA_REQUIRED",
        "reason": reason.as_str(),
        "category": category.as_str(),
        "error": message,
    });
    if reason == WindowReason::Invalidated {
        crate::audit::set_failure_reason("mfa_step_up_invalidated");
        crate::audit::mark_security_event();
        let mut response = (StatusCode::FORBIDDEN, Json(body)).into_response();
        crate::audit::AuditErrorInfo::attach(&mut response, "MFA_REQUIRED", &message);
        return response;
    }
    json_error(StatusCode::FORBIDDEN, "MFA_REQUIRED", &message, body)
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

#[derive(Debug)]
pub(crate) struct CheckedSubject {
    pub user_id: uuid::Uuid,
    pub epoch: i64,
}

/// 账号复核失败。POST step-up 和守卫共用，避免再写一套判断。
#[derive(Debug)]
pub(crate) enum SubjectLoadError {
    Unauthorized { stale: bool },
    NotEnabled,
    Locked { retry_after_secs: i64 },
    Unavailable,
}

impl SubjectLoadError {
    pub(crate) fn into_response(self) -> Response {
        match self {
            Self::Unauthorized { stale } => unauthorized(stale),
            Self::NotEnabled => not_enabled(),
            Self::Locked { retry_after_secs } => locked(retry_after_secs),
            Self::Unavailable => unavailable(),
        }
    }
}

pub(crate) async fn load_checked_subject(
    pool: &sqlx::PgPool,
    password_policy_cache: &crate::services::PasswordPolicyCache,
    claims: &AdminClaims,
) -> Result<CheckedSubject, SubjectLoadError> {
    if uuid::Uuid::parse_str(&claims.sid).is_err() {
        return Err(SubjectLoadError::Unauthorized { stale: false });
    }
    let user_id =
        extract_user_id(claims).map_err(|_| SubjectLoadError::Unauthorized { stale: false })?;
    let row = crate::repositories::admin::get_step_up_subject(pool, user_id)
        .await
        .map_err(|_| SubjectLoadError::Unavailable)?
        .ok_or(SubjectLoadError::Unauthorized { stale: false })?;
    let now = Utc::now();
    let policy = password_policy_cache.get().await;
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
        Err(SubjectDenial::Unauthorized) => Err(SubjectLoadError::Unauthorized { stale: false }),
        Err(SubjectDenial::SessionStale) => Err(SubjectLoadError::Unauthorized { stale: true }),
        Err(SubjectDenial::NotEnabled) => Err(SubjectLoadError::NotEnabled),
        Err(SubjectDenial::Locked) => Err(SubjectLoadError::Locked { retry_after_secs }),
    }
}

/// 要求当前会话的这一类提权窗口有效。窗口在有效期内可重复使用。
///
/// # Errors
///
/// 会话无效返回 401；未绑定、锁定或没有窗口返回 403；依赖故障返回 503。
pub async fn require_category(
    state: &AppState,
    claims: &AdminClaims,
    category: Category,
) -> Result<CategoryVerified, Box<Response>> {
    let checked = load_checked_subject(&state.admin_db, &state.password_policy_cache, claims)
        .await
        .map_err(|error| Box::new(error.into_response()))?;
    let window = step_up::load_category_window(&state.redis, &claims.sid, category)
        .await
        .map_err(|_| Box::new(unavailable()))?
        .ok_or_else(|| Box::new(step_up_required(category, WindowReason::None)))?;
    let now = Utc::now().timestamp();
    match step_up::classify_window(&window, &checked.user_id.to_string(), checked.epoch, now) {
        Ok(()) => {
            crate::audit::add_detail("mfa", serde_json::json!(category.as_str()));
            crate::audit::add_detail("category", serde_json::json!(category.as_str()));
            crate::audit::add_detail("step_up_opened_at", serde_json::json!(window.opened_at));
            crate::audit::add_detail("step_up_method", serde_json::json!(window.method.as_str()));
            Ok(CategoryVerified(category))
        }
        Err(WindowMiss::Expired) => {
            Err(Box::new(step_up_required(category, WindowReason::Expired)))
        }
        Err(WindowMiss::Invalidated) => {
            if let Err(error) = step_up::delete_category_if_match(
                &state.redis,
                &claims.sid,
                category,
                &window.uid,
                window.epoch,
            )
            .await
            {
                tracing::warn!("failed to drop invalidated step-up window: {error}");
            }
            Err(Box::new(step_up_required(
                category,
                WindowReason::Invalidated,
            )))
        }
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

    fn assert_category(pairs: &[(&str, &str)], category: &str) {
        for (source, name) in pairs {
            let body = handler_source(source, name);
            assert!(
                body.contains("require_category(") && body.contains(category),
                "{name} must require {category}"
            );
        }
    }

    #[test]
    fn certificate_handlers_require_their_category() {
        let certs = include_str!("../proxy/cert/certificates.rs");
        let authorization = include_str!("../proxy/cert/authorization.rs");
        assert_category(
            &[
                (certs, "issue_certificate_proxy"),
                (authorization, "import_license_proxy"),
            ],
            "Category::Certificate",
        );
        assert_eq!(certs.matches("require_mfa = \"certificate\"").count(), 2);
        let full = handler_source(certs, "get_full_certificate_proxy");
        let exists = full
            .find("CertificateLookup::Present")
            .expect("certificate existence check");
        let full_mfa = full
            .find("require_category(")
            .expect("full certificate mfa");
        assert!(exists < full_mfa, "full certificate must exist before MFA");
        assert!(full.contains("Category::Certificate"));
        let issue = handler_source(certs, "issue_certificate_proxy");
        let json = issue.find("请求体不是合法 JSON").expect("json validation");
        let issue_mfa = issue.find("require_category(").expect("issue mfa");
        assert!(json < issue_mfa, "issue must validate JSON before MFA");
    }

    #[test]
    fn admin_handlers_require_their_category() {
        let crud = include_str!("admin_management/crud.rs");
        let password = include_str!("admin_management/password.rs");
        let status = include_str!("admin_management/status.rs");
        let admin_mfa = include_str!("admin_management/mfa.rs");
        assert_category(
            &[
                (crud, "create_admin"),
                (crud, "delete_admin"),
                (crud, "update_admin"),
                (password, "force_password_reset"),
                (password, "reset_admin_password"),
                (status, "set_admin_status"),
                (admin_mfa, "reset_admin_mfa"),
            ],
            "Category::Admin",
        );
        let update = handler_source(crud, "update_admin");
        let lifts = update
            .find("admin_update_lifts_mfa_requirement")
            .expect("lifts query");
        let update_mfa = update.find("require_category(").expect("update mfa");
        assert!(lifts < update_mfa, "update must resolve lifts before MFA");
        let reset_password = handler_source(password, "reset_admin_password");
        let precheck = reset_password
            .find("precheck_password_reset")
            .expect("password precheck");
        let reset_mfa = reset_password.find("require_category(").expect("reset mfa");
        assert!(precheck < reset_mfa);
    }

    #[test]
    fn mfa_handlers_require_their_category() {
        let disable = include_str!("mfa/disable.rs");
        let backup_codes = include_str!("mfa/backup_codes.rs");
        let mfa_settings = include_str!("mfa_settings_handlers.rs");
        assert!(disable.contains("require_category(") && disable.contains("Category::Mfa"));
        assert!(
            backup_codes.contains("require_category(") && backup_codes.contains("Category::Mfa")
        );
        let settings_update = handler_source(mfa_settings, "update_mfa_settings");
        assert!(
            settings_update.contains("require_category(")
                && settings_update.contains("Category::Mfa")
        );
        let role = settings_update
            .find("仅管理员可修改 MFA 设置")
            .expect("mfa settings role");
        let settings_mfa = settings_update
            .find("require_category(")
            .expect("mfa settings category");
        assert!(role < settings_mfa);
    }

    #[test]
    fn key_handlers_require_their_category() {
        let crypto = include_str!("tenant_crypto_handlers.rs");
        let api_keys = include_str!("api_key_handlers.rs");
        assert_category(
            &[
                (api_keys, "create_api_key"),
                (api_keys, "revoke_api_key"),
                (api_keys, "update_api_key"),
                (crypto, "update_crypto_profile"),
                (crypto, "create_crypto_key"),
                (crypto, "rotate_crypto_key"),
                (crypto, "activate_crypto_key"),
                (crypto, "deprecate_crypto_key"),
                (crypto, "disable_crypto_key"),
                (crypto, "delete_crypto_key"),
                (crypto, "create_request_key"),
                (crypto, "retire_request_key"),
            ],
            "Category::Key",
        );
        let export = handler_source(crypto, "export_crypto_data");
        assert!(export.contains("is_migration_export") && export.contains("Category::Key"));
    }

    #[test]
    fn finance_handlers_require_their_category() {
        let billing = include_str!("billing_handlers.rs");
        let account = include_str!("product/account.rs");
        assert_category(
            &[
                (account, "recharge_tenant_account"),
                (account, "refund_tenant_account"),
                (billing, "delete_bill"),
                (billing, "delete_reconciliation_discrepancy"),
                (billing, "create_adjustment"),
                (billing, "generate_bill"),
                (billing, "create_bill_adjustment"),
                (billing, "void_bill_adjustment"),
                (billing, "generate_bill_async"),
            ],
            "Category::Finance",
        );
        assert!(!handler_source(billing, "update_bill_status").contains("require_category("));
        assert!(
            !handler_source(billing, "update_reconciliation_settings")
                .contains("require_category(")
        );
    }

    #[test]
    fn platform_handlers_require_their_category() {
        let cc_config = include_str!("config_center/config.rs");
        let cc_history = include_str!("config_center/history.rs");
        let cc_import = include_str!("config_center/import_export.rs");
        let settings = include_str!("admin/settings.rs");
        let password_policy = include_str!("password_policy_handlers.rs");
        let tenants = include_str!("tenant_handlers.rs");
        let ratelimit = include_str!("ratelimit_command_handlers.rs");
        let upstream = include_str!("upstream_connector_handlers.rs");
        assert_category(
            &[
                (cc_config, "create_config"),
                (cc_config, "update_config"),
                (cc_config, "delete_config"),
                (cc_history, "rollback"),
                (cc_import, "import_configs"),
                (cc_import, "export_configs"),
                (settings, "update_system_settings"),
                (password_policy, "update_password_policy"),
                (upstream, "rollback"),
                (ratelimit, "create_control_command"),
                (tenants, "delete_tenant"),
            ],
            "Category::Platform",
        );
        let publish = handler_source(upstream, "publish_draft");
        assert!(publish.contains("force_publish") && publish.contains("Category::Platform"));
        let export_configs = handler_source(cc_import, "export_configs");
        let config_admin = export_configs
            .find("require_config_admin")
            .expect("config admin");
        let export_mfa = export_configs
            .find("require_category(")
            .expect("export mfa");
        assert!(config_admin < export_mfa);
        assert!(!handler_source(tenants, "create_tenant").contains("require_category("));
        assert!(!handler_source(tenants, "update_tenant").contains("require_category("));
    }

    #[test]
    fn legacy_mfa_header_and_columns_are_gone() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let roots = [
            root.join("mazbri/mazbri-bin"),
            root.join("mazbri/mazbri-web/src"),
            root.join("mazbri/mazbri-libs"),
        ];
        let needles = [
            concat!("x-mazbri-mfa-", "code"),
            concat!("require_mfa_", "code"),
            concat!("mfa_", "verified_at"),
        ];
        let mut hits = Vec::new();
        for base in roots {
            scan_legacy(&base, &needles, &mut hits);
        }
        assert!(hits.is_empty(), "legacy MFA markers remain: {hits:?}");
    }

    fn scan_legacy(dir: &std::path::Path, needles: &[&str], hits: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == "target" || name == "node_modules" || name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                scan_legacy(&path, needles, hits);
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for needle in needles {
                if text.contains(needle) {
                    hits.push(format!("{}: {needle}", path.display()));
                }
            }
        }
    }
}
