use super::super::AppState;
use super::super::types::{ChangePasswordRequest, FirstLoginChangePasswordRequest};
use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::error::ApiError;
use crate::middleware::auth::json_with_session_cookies;
use crate::response::ApiResponse;
use crate::services::admin;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
pub async fn first_login_change_password(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<FirstLoginChangePasswordRequest>,
) -> Result<Response, ApiError> {
    let user_id = uuid::Uuid::parse_str(&claims.user_id)
        .map_err(|e| ApiError::internal(format!("Token 中的用户 ID 无效: {}", e)))?;
    let data = admin::first_login_change_password(
        &state.admin_db,
        &state.auth_state.jwt_secret,
        &state.password_policy_cache,
        state.config.auth.jwt_expiration_secs,
        user_id,
        &req,
    )
    .await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    state.audit.log(create_audit_log(
        user_id,
        AuditAction::PasswordChangeFirstLogin,
        AuditResourceType::User,
        user_id.to_string(),
        Some(serde_json::json!({ "username" : claims.sub, "first_login" : true })),
        client_ip,
    ));
    Ok(json_with_session_cookies(
        data,
        Some(state.config.auth.refresh_token_expiration_secs),
    ))
}
pub async fn change_password(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = uuid::Uuid::parse_str(&claims.user_id)
        .map_err(|e| ApiError::internal(format!("Token 中的用户 ID 无效: {}", e)))?;
    let data = admin::change_password(&state.admin_db, &state.password_policy_cache, user_id, &req)
        .await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    state.audit.log(create_audit_log(
        user_id,
        AuditAction::PasswordChange,
        AuditResourceType::User,
        user_id.to_string(),
        Some(serde_json::json!({ "username" : claims.sub })),
        client_ip,
    ));
    Ok(ApiResponse::ok(data))
}
