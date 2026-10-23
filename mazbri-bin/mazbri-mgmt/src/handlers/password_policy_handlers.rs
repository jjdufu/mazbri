use super::AppState;
use crate::audit::extract_client_ip;
use crate::middleware::auth::{AdminClaims, extract_user_id};
use crate::services::password_policy as service;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
};
use serde::Deserialize;
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdatePasswordPolicyRequest {
    pub min_length: Option<i32>,
    pub max_length: Option<i32>,
    pub require_uppercase: Option<bool>,
    pub require_lowercase: Option<bool>,
    pub require_digit: Option<bool>,
    pub require_special_char: Option<bool>,
    pub special_chars: Option<String>,
    pub max_age_days: Option<i32>,
    pub warning_days: Option<i32>,
    pub prevent_reuse_count: Option<i32>,
    pub max_login_attempts: Option<i32>,
    pub lockout_duration_minutes: Option<i32>,
}
pub async fn get_password_policy(State(state): State<AppState>) -> axum::response::Response {
    service::get_policy(&state.admin_db).await
}
pub async fn update_password_policy(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<UpdatePasswordPolicyRequest>,
) -> axum::response::Response {
    use crate::response::ApiResponse;
    use axum::http::StatusCode;
    if claims.role != "R_ADMIN" {
        return ApiResponse::err(StatusCode::FORBIDDEN, "仅管理员可修改密码策略");
    }
    let audit_user_id = match extract_user_id(&claims) {
        Ok(user_id) => user_id,
        Err(e) => {
            tracing::warn!("Invalid user_id in claims for audit: {:?}", e);
            return ApiResponse::err(StatusCode::UNAUTHORIZED, "令牌中的用户标识无效");
        }
    };
    let client_ip = extract_client_ip(&headers, Some(addr));
    let input = service::UpdatePasswordPolicyInput {
        min_length: payload.min_length,
        max_length: payload.max_length,
        require_uppercase: payload.require_uppercase,
        require_lowercase: payload.require_lowercase,
        require_digit: payload.require_digit,
        require_special_char: payload.require_special_char,
        special_chars: payload.special_chars,
        max_age_days: payload.max_age_days,
        warning_days: payload.warning_days,
        prevent_reuse_count: payload.prevent_reuse_count,
        max_login_attempts: payload.max_login_attempts,
        lockout_duration_minutes: payload.lockout_duration_minutes,
    };
    let resp = service::update_policy(
        &state.admin_db,
        &state.audit,
        &claims.user_id,
        audit_user_id,
        &input,
        client_ip,
    )
    .await;
    if resp.status().is_success() {
        state.password_policy_cache.invalidate().await;
    }
    resp
}
