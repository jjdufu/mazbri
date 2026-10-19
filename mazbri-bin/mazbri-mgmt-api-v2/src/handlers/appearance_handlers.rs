use super::AppState;
use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::middleware::auth::AdminClaims;
use crate::response::ApiResponse;
use crate::services::appearance as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use std::net::SocketAddr;
pub use svc::AppearanceSettings;
pub use svc::UpdateAppearanceRequest;
pub async fn get_appearance_settings(State(state): State<AppState>) -> axum::response::Response {
    match svc::get_settings(&state.admin_db).await {
        Ok(settings) => ApiResponse::ok(settings).into_response(),
        Err(msg) => ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, msg),
    }
}
pub async fn update_appearance_settings(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<UpdateAppearanceRequest>,
) -> axum::response::Response {
    if claims.role != "R_ADMIN" {
        return ApiResponse::err(StatusCode::FORBIDDEN, "仅管理员可修改外观设置");
    }
    match svc::validate_update(&payload) {
        Ok(()) => {}
        Err(msg) => return ApiResponse::err(StatusCode::BAD_REQUEST, msg),
    }
    match svc::update_settings(&state.admin_db, &claims.user_id, &claims.sub, &payload).await {
        Ok(settings) => {
            state.audit.log(create_audit_log(
                uuid::Uuid::parse_str(&claims.user_id)
                    .ok()
                    .unwrap_or_else(uuid::Uuid::nil),
                AuditAction::AppearanceUpdate,
                AuditResourceType::System,
                Some("appearance".to_string()),
                Some(serde_json::json!(
                    { "scope" : "appearance_settings", "changes" : {
                    "system_theme_mode" : payload.system_theme_mode,
                    "system_theme_color" : payload.system_theme_color,
                    "container_width" : payload.container_width,
                    "box_border_mode" : payload.box_border_mode,
                    "menu_open_width" : payload.menu_open_width, "show_work_tab"
                    : payload.show_work_tab, "show_menu_button" : payload
                    .show_menu_button, "show_fast_enter" : payload
                    .show_fast_enter, "show_refresh_button" : payload
                    .show_refresh_button, "show_crumbs" : payload.show_crumbs,
                    "show_language" : payload.show_language, "show_nprogress" :
                    payload.show_nprogress, "watermark_visible" : payload
                    .watermark_visible, "tab_style" : payload.tab_style,
                    "page_transition" : payload.page_transition, "custom_radius"
                    : payload.custom_radius, "dark_theme_style" : payload.dark_theme_style, } }
                )),
                extract_client_ip(&headers, Some(addr)),
            ));
            ApiResponse::ok(settings).into_response()
        }
        Err(msg) => ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, msg),
    }
}
pub async fn reset_appearance_settings(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> axum::response::Response {
    if claims.role != "R_ADMIN" {
        return ApiResponse::err(StatusCode::FORBIDDEN, "仅管理员可重置外观设置");
    }
    match svc::reset_settings(&state.admin_db, &claims.user_id, &claims.sub).await {
        Ok(settings) => {
            state.audit.log(create_audit_log(
                uuid::Uuid::parse_str(&claims.user_id)
                    .ok()
                    .unwrap_or_else(uuid::Uuid::nil),
                AuditAction::AppearanceReset,
                AuditResourceType::System,
                Some("appearance".to_string()),
                Some(serde_json::json!(
                    { "scope" : "appearance_settings", "action" :
                    "reset_to_default", }
                )),
                extract_client_ip(&headers, Some(addr)),
            ));
            ApiResponse::ok(settings).into_response()
        }
        Err(msg) => ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, msg),
    }
}
