use super::super::AppState;
use super::super::types::{SystemSettingsUpdate, UserSettingsUpdate};
use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::error::ApiError;
use crate::middleware::auth::{AdminClaims, can_access_settings};
use crate::response::ApiResponse;
use crate::services::admin;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
    response::IntoResponse,
};
use std::net::SocketAddr;
use tracing::info;
pub async fn get_system_settings(
    State(state): State<AppState>,
    Extension(_claims): Extension<AdminClaims>,
) -> Result<impl IntoResponse, ApiError> {
    let data = admin::get_system_settings(&state.admin_db).await?;
    Ok(ApiResponse::ok(data))
}
pub async fn update_system_settings(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<SystemSettingsUpdate>,
) -> Result<impl IntoResponse, ApiError> {
    if !can_access_settings(&claims) {
        return Err(ApiError::Forbidden);
    }
    info!("Updating system settings (by {})", claims.sub);
    let data = admin::update_system_settings(state.admin_db.clone(), &req).await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    let audit_user_id = uuid::Uuid::parse_str(&claims.user_id).unwrap_or(uuid::Uuid::nil());
    state.audit.log(create_audit_log(
        audit_user_id,
        AuditAction::SettingsUpdate,
        AuditResourceType::System,
        Some("system".to_string()),
        Some(serde_json::json!({
            "scope": "system_settings",
            "changes": {
                "site_name": req.site_name,
                "site_url": req.site_url,
                "tenant_gateway_url": req.tenant_gateway_url,
                "enable_audit_log": req.enable_audit_log,
                "session_timeout": req.session_timeout,
                "max_upload_size": req.max_upload_size,
            }
        })),
        client_ip,
    ));
    Ok(ApiResponse::ok(data))
}
pub async fn get_user_settings(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let data = admin::get_user_settings(&state.admin_db).await?;
    Ok(ApiResponse::ok(data))
}
pub async fn update_user_settings(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<UserSettingsUpdate>,
) -> Result<impl IntoResponse, ApiError> {
    info!("Updating user settings");
    let user_id = uuid::Uuid::parse_str(&claims.user_id)
        .map_err(|e| ApiError::internal(format!("Token 中的用户 ID 无效: {}", e)))?;
    let data = admin::update_user_settings(state.admin_db.clone(), &req).await?;
    let client_ip = extract_client_ip(&headers, Some(addr));
    state.audit.log(create_audit_log(
        user_id,
        AuditAction::UserSettingsUpdate,
        AuditResourceType::User,
        user_id.to_string(),
        Some(serde_json::json!({ "language" : &req.language, "email" : &req.email })),
        client_ip,
    ));
    Ok(ApiResponse::ok(data))
}
