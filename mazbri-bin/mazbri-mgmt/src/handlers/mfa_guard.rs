//! 按功能要求 MFA 的统一守卫。
//!
//! 敏感操作（资金、删除、强制下发、密钥与证书、超级管理员配置等）在处理器里调用
//! [`require_mfa_code`]：
//!
//! - 每次请求都从数据库读取调用者当前的 `mfa_enabled`（不信任令牌中的任何 MFA 状态）；
//! - 未绑定 MFA：403 `MFA_NOT_ENABLED`，响应携带 `setup_path`，前端引导到账号安全页绑定；
//! - 已绑定但请求未携带 / 携带错误的 `x-mazbri-mfa-code`：403 `MFA_REQUIRED`，前端弹出验证码框后重试。
//!
//! 不存在任何角色豁免（包括超级管理员）。
use crate::handlers::AppState;
use crate::middleware::auth::{AdminClaims, extract_user_id};
use axum::{
    Json,
    extract::FromRequestParts,
    http::{HeaderMap, StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
const MFA_HEADER: &str = "x-mazbri-mfa-code";
/// 未启用 MFA 时前端引导用户前往的自助绑定页面。
pub const MFA_SETUP_PATH: &str = "/account/security";
const NOT_ENABLED_MESSAGE: &str = "此操作需要 MFA 二次验证，请先在「账号安全」页面绑定并启用 MFA";

/// 守卫拒绝原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateFailure {
    /// 账号未绑定 MFA（或绑定数据不完整）。
    NotEnabled,
    /// 已绑定 MFA，但请求没有携带验证码。
    CodeMissing,
    /// 验证码错误。
    CodeInvalid,
    /// 读取 MFA 状态或解密失败。
    CheckError,
}

impl GateFailure {
    /// 审计失败原因代码（不含验证码本身）。
    const fn audit_reason(self) -> &'static str {
        match self {
            Self::NotEnabled => "mfa_not_enabled",
            Self::CodeMissing => "mfa_required",
            Self::CodeInvalid => "mfa_invalid_code",
            Self::CheckError => "mfa_check_error",
        }
    }

    fn into_response(self) -> Response {
        crate::audit::set_failure_reason(self.audit_reason());
        crate::audit::mark_security_event();
        let (code, message, body) = match self {
            Self::NotEnabled => (
                "MFA_NOT_ENABLED",
                NOT_ENABLED_MESSAGE,
                serde_json::json!({
                    "success": false,
                    "code": "MFA_NOT_ENABLED",
                    "error": NOT_ENABLED_MESSAGE,
                    "setup_path": MFA_SETUP_PATH,
                }),
            ),
            Self::CodeMissing | Self::CodeInvalid | Self::CheckError => {
                let message = if self == Self::CodeMissing {
                    "MFA verification required"
                } else {
                    "MFA verification failed"
                };
                (
                    "MFA_REQUIRED",
                    message,
                    serde_json::json!({ "success": false, "code": "MFA_REQUIRED", "error": message }),
                )
            }
        };
        let mut response = (StatusCode::FORBIDDEN, Json(body)).into_response();
        crate::audit::AuditErrorInfo::attach(&mut response, code, message);
        response
    }
}

/// 只依据数据库中的当前 MFA 状态和请求头做判定（不访问外部依赖），返回待校验的
/// 加密密钥与验证码。
fn precheck(
    mfa_enabled: bool,
    mfa_secret: Option<String>,
    headers: &HeaderMap,
) -> Result<(String, &str), GateFailure> {
    let secret = mfa_secret
        .filter(|_| mfa_enabled)
        .ok_or(GateFailure::NotEnabled)?;
    let code = headers
        .get(MFA_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(GateFailure::CodeMissing)?;
    Ok((secret, code))
}

/// 要求调用者已绑定 MFA 并在 `x-mazbri-mfa-code` 中提供正确的 TOTP 验证码。
///
/// # Errors
///
/// 未绑定 MFA 返回 403 `MFA_NOT_ENABLED`（带 `setup_path`）；缺少或错误的验证码、
/// 状态读取失败返回 403 `MFA_REQUIRED`；令牌中无有效用户 ID 返回 401。
pub async fn require_mfa_code(
    state: &AppState,
    claims: &AdminClaims,
    headers: &HeaderMap,
) -> Result<(), Box<Response>> {
    let user_id = extract_user_id(claims).map_err(IntoResponse::into_response)?;
    let mfa_state = crate::repositories::mfa::get_mfa_verify_state(&state.admin_db, user_id)
        .await
        .map_err(|_| GateFailure::CheckError.into_response())?
        .ok_or_else(|| GateFailure::NotEnabled.into_response())?;
    let (encrypted_secret, code) = precheck(mfa_state.mfa_enabled, mfa_state.mfa_secret, headers)
        .map_err(GateFailure::into_response)?;
    let mfa_service = state
        .mfa_cache
        .get()
        .await
        .map_err(|_| GateFailure::CheckError.into_response())?;
    let secret = mfa_service
        .decrypt_secret(&encrypted_secret)
        .map_err(|_| GateFailure::CheckError.into_response())?;
    if !mfa_service.verify_totp(&secret, code) {
        return Err(Box::new(GateFailure::CodeInvalid.into_response()));
    }
    Ok(())
}

/// 处理器参数形式的 MFA 守卫：整条接口无条件要求 MFA 时，在处理器参数里加上
/// `_mfa: MfaVerified` 即可（必须位于 `Extension<AdminClaims>` 之后、请求体之前）。
///
/// 角色校验由 `route_access` 中间件先行完成（默认拒绝），因此无权限的调用者不会走到这里。
/// 仅在满足特定条件（如强制发布、迁移导出）才要求 MFA 的接口，直接在处理器里调用
/// [`require_mfa_code`]。
#[derive(Debug, Clone, Copy)]
pub struct MfaVerified;

impl FromRequestParts<AppState> for MfaVerified {
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
        require_mfa_code(state, &claims, &parts.headers)
            .await
            .map_err(|response| *response)?;
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with(code: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(code) = code {
            headers.insert(MFA_HEADER, HeaderValue::from_str(code).expect("header"));
        }
        headers
    }

    async fn body_of(response: Response) -> (StatusCode, serde_json::Value) {
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        (status, serde_json::from_slice(&bytes).expect("json"))
    }

    #[test]
    fn unbound_account_is_not_enabled_regardless_of_code() {
        for code in [None, Some("123456")] {
            let headers = headers_with(code);
            assert_eq!(
                precheck(false, None, &headers),
                Err(GateFailure::NotEnabled)
            );
            // 数据不一致（标记启用但无密钥 / 有残留密钥但未启用）都按未启用处理。
            assert_eq!(precheck(true, None, &headers), Err(GateFailure::NotEnabled));
            assert_eq!(
                precheck(false, Some("enc".to_string()), &headers),
                Err(GateFailure::NotEnabled)
            );
        }
    }

    #[test]
    fn bound_account_without_code_requires_code() {
        for code in [None, Some(""), Some("   ")] {
            assert_eq!(
                precheck(true, Some("enc".to_string()), &headers_with(code)),
                Err(GateFailure::CodeMissing)
            );
        }
    }

    #[test]
    fn bound_account_with_code_proceeds_to_totp_check() {
        let headers = headers_with(Some(" 654321 "));
        assert_eq!(
            precheck(true, Some("enc".to_string()), &headers),
            Ok(("enc".to_string(), "654321"))
        );
    }

    #[tokio::test]
    async fn not_enabled_response_points_to_account_security() {
        let (status, body) = body_of(GateFailure::NotEnabled.into_response()).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["code"], "MFA_NOT_ENABLED");
        assert_eq!(body["setup_path"], "/account/security");
        assert_eq!(body["success"], false);
        // 文案不再写死某个功能（此前固定为“后再充值”）。
        assert!(!body["error"].as_str().expect("error").contains("充值"));
    }

    #[tokio::test]
    async fn code_failures_keep_mfa_required_contract() {
        for failure in [
            GateFailure::CodeMissing,
            GateFailure::CodeInvalid,
            GateFailure::CheckError,
        ] {
            let (status, body) = body_of(failure.into_response()).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(body["code"], "MFA_REQUIRED");
            assert!(body.get("setup_path").is_none());
        }
    }

    /// 取出 `pub async fn {name}(` 开始到函数体结束的源码片段。
    fn handler_source<'a>(source: &'a str, name: &str) -> &'a str {
        let start = source
            .find(&format!("pub async fn {name}("))
            .unwrap_or_else(|| panic!("handler {name} not found"));
        let rest = &source[start..];
        let end = rest.find("\n}\n").unwrap_or(rest.len());
        &rest[..end]
    }

    /// 受 MFA 保护的功能清单（防止重构时漏掉守卫）。
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

        let unconditional: &[(&str, &str)] = &[
            (crud, "create_admin"),
            (crud, "update_admin"),
            (crud, "delete_admin"),
            (password, "reset_admin_password"),
            (password, "force_password_reset"),
            (status, "set_admin_status"),
            (admin_mfa, "reset_admin_mfa"),
            (cc_config, "create_config"),
            (cc_config, "update_config"),
            (cc_config, "delete_config"),
            (cc_history, "rollback"),
            (cc_import, "import_configs"),
            (settings, "update_system_settings"),
            (password_policy, "update_password_policy"),
            (mfa_settings, "update_mfa_settings"),
            (tenants, "delete_tenant"),
            (billing, "delete_reconciliation_discrepancy"),
            (billing, "delete_bill"),
            (ratelimit, "create_control_command"),
            (upstream, "rollback"),
            (crypto, "rotate_crypto_key"),
            (api_keys, "revoke_api_key"),
        ];
        for (source, name) in unconditional {
            assert!(
                handler_source(source, name).contains("MfaVerified"),
                "{name} must take MfaVerified"
            );
        }

        let inline: &[(&str, &str)] = &[
            (billing, "create_adjustment"),
            (billing, "generate_bill"),
            (billing, "generate_bill_async"),
            (billing, "create_bill_adjustment"),
            (billing, "void_bill_adjustment"),
            (account, "recharge_tenant_account"),
            (account, "refund_tenant_account"),
            (certs, "issue_certificate_proxy"),
        ];
        for (source, name) in inline {
            assert!(
                handler_source(source, name).contains("require_mfa_code("),
                "{name} must call require_mfa_code"
            );
        }

        // 条件守卫：只在强制发布 / 迁移导出时要求 MFA。
        let publish = handler_source(upstream, "publish_draft");
        assert!(publish.contains("force_publish") && publish.contains("require_mfa_code("));
        let export = handler_source(crypto, "export_crypto_data");
        assert!(export.contains("is_migration_export") && export.contains("require_mfa_code("));
        // 证书吊销由宏生成：显式声明 require_mfa = true；续期不要求。
        assert!(certs.contains("\"Admin {} revoking certificate {}\", require_mfa = true"));
        assert!(certs.contains("\"Admin {} renewing certificate {}\", require_mfa = false"));
    }
}
