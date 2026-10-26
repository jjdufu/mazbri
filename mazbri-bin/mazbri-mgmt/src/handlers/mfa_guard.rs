use crate::handlers::AppState;
use crate::middleware::auth::{AdminClaims, extract_user_id, is_admin};
use axum::{
    Json,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
const MFA_HEADER: &str = "x-mazbri-mfa-code";
/// 二次验证失败响应；`reason` 为审计失败原因代码（不含验证码本身）。
fn mfa_error_response(code: &str, message: &str, reason: &str) -> Response {
    crate::audit::set_failure_reason(reason);
    crate::audit::mark_security_event();
    let mut response = (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({ "success" : false, "code" : code, "error" : message, })),
    )
        .into_response();
    crate::audit::AuditErrorInfo::attach(&mut response, code, message);
    response
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn require_mfa_code(
    state: &AppState,
    claims: &AdminClaims,
    headers: &HeaderMap,
) -> Result<(), Box<Response>> {
    if is_admin(claims) {
        return Ok(());
    }
    let user_id = extract_user_id(claims).map_err(IntoResponse::into_response)?;
    let mfa_state = crate::repositories::mfa::get_mfa_verify_state(&state.admin_db, user_id)
        .await
        .map_err(|_| {
            mfa_error_response("MFA_REQUIRED", "MFA verification failed", "mfa_check_error")
        })?
        .ok_or_else(|| {
            mfa_error_response(
                "MFA_REQUIRED",
                "MFA verification required",
                "mfa_not_enabled",
            )
        })?;
    if !mfa_state.mfa_enabled {
        return Err(Box::new(mfa_error_response(
            "MFA_NOT_ENABLED",
            "当前账号未启用 MFA，请联系管理员先启用 MFA 后再充值",
            "mfa_not_enabled",
        )));
    }
    let Some(encrypted_secret) = mfa_state.mfa_secret else {
        return Err(Box::new(mfa_error_response(
            "MFA_NOT_ENABLED",
            "当前账号未启用 MFA，请联系管理员先启用 MFA 后再充值",
            "mfa_not_enabled",
        )));
    };
    let code = headers
        .get(MFA_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            mfa_error_response("MFA_REQUIRED", "MFA verification required", "mfa_required")
        })?;
    let mfa_service = state.mfa_cache.get().await.map_err(|_| {
        mfa_error_response("MFA_REQUIRED", "MFA verification failed", "mfa_check_error")
    })?;
    let secret = mfa_service.decrypt_secret(&encrypted_secret).map_err(|_| {
        mfa_error_response("MFA_REQUIRED", "MFA verification failed", "mfa_check_error")
    })?;
    if !mfa_service.verify_totp(&secret, code) {
        return Err(Box::new(mfa_error_response(
            "MFA_REQUIRED",
            "MFA verification failed",
            "mfa_invalid_code",
        )));
    }
    Ok(())
}
