use super::AppState;
use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::middleware::auth::AdminClaims;
use crate::response::ApiResponse;
use crate::services::mfa_settings_service as service;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
#[derive(Debug, Serialize, Deserialize)]
pub struct MfaSettingsResponse {
    pub mfa_enabled_globally: bool,
    pub mfa_required_for_all: bool,
    pub mfa_required_for_roles: Vec<String>,
    pub totp_issuer: String,
    pub totp_digits: i32,
    pub totp_period: i32,
    pub backup_codes_count: i32,
    pub updated_at: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateMfaSettingsRequest {
    pub mfa_enabled_globally: Option<bool>,
    pub mfa_required_for_all: Option<bool>,
    pub mfa_required_for_roles: Option<Vec<String>>,
    pub totp_issuer: Option<String>,
    pub totp_digits: Option<i32>,
    pub totp_period: Option<i32>,
    pub backup_codes_count: Option<i32>,
}
pub async fn get_mfa_settings(State(state): State<AppState>) -> axum::response::Response {
    service::get_settings(&state.admin_db).await
}
pub async fn update_mfa_settings(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<UpdateMfaSettingsRequest>,
) -> axum::response::Response {
    if claims.role != "R_ADMIN" {
        return ApiResponse::err(StatusCode::FORBIDDEN, "仅管理员可修改 MFA 设置");
    }
    let mfa_enabled_globally = payload.mfa_enabled_globally;
    let mfa_required_for_all = payload.mfa_required_for_all;
    let mfa_required_for_roles = payload.mfa_required_for_roles.clone();
    let totp_issuer = payload.totp_issuer.clone();
    let totp_digits = payload.totp_digits;
    let totp_period = payload.totp_period;
    let backup_codes_count = payload.backup_codes_count;
    let input = service::UpdateMfaSettingsInput {
        mfa_enabled_globally,
        mfa_required_for_all,
        mfa_required_for_roles,
        totp_issuer,
        totp_digits,
        totp_period,
        backup_codes_count,
    };
    let resp =
        service::update_settings(&state.admin_db, &claims.sub, &claims.user_id, &input).await;
    if resp.status().is_success() {
        state.mfa_settings_cache.invalidate().await;
        state.mfa_cache.invalidate().await;
        state.audit.log(create_audit_log(
            uuid::Uuid::parse_str(&claims.user_id)
                .ok()
                .unwrap_or_else(uuid::Uuid::nil),
            AuditAction::MfaSettingsUpdate,
            AuditResourceType::System,
            Some("mfa_settings".to_string()),
            Some(serde_json::json!(
                    { "scope" : "mfa_settings", "changes" : {
                "mfa_enabled_globally" : input.mfa_enabled_globally,
                "mfa_required_for_all" : input.mfa_required_for_all,
                "mfa_required_for_roles" : input.mfa_required_for_roles,
                "totp_issuer" : input.totp_issuer, "totp_digits" : input.totp_digits,
                "totp_period" : input.totp_period, "backup_codes_count" :
                input.backup_codes_count, } }
            )),
            extract_client_ip(&headers, Some(addr)),
        ));
    }
    resp
}
