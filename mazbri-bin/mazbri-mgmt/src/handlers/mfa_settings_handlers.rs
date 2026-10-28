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
    pub totp_issuer: String,
    pub totp_digits: i32,
    pub totp_period: i32,
    pub backup_codes_count: i32,
    pub updated_at: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateMfaSettingsRequest {
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
    let input = service::UpdateMfaSettingsInput {
        totp_issuer: payload.totp_issuer.clone(),
        totp_digits: payload.totp_digits,
        totp_period: payload.totp_period,
        backup_codes_count: payload.backup_codes_count,
    };
    if let Some(error) = service::validate_update_fields(&input) {
        return error;
    }
    if let Err(response) = crate::handlers::mfa_guard::require_fresh_mfa(
        &state,
        &claims,
        crate::services::mfa::step_up::FreshPurpose::MfaSettingsUpdate,
    )
    .await
    {
        return *response;
    }
    let totp_issuer = payload.totp_issuer.clone();
    let totp_digits = payload.totp_digits;
    let totp_period = payload.totp_period;
    let backup_codes_count = payload.backup_codes_count;
    let input = service::UpdateMfaSettingsInput {
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
                "totp_issuer" : input.totp_issuer, "totp_digits" : input.totp_digits,
                "totp_period" : input.totp_period, "backup_codes_count" :
                input.backup_codes_count, } }
            )),
            extract_client_ip(&headers, Some(addr)),
        ));
    }
    resp
}
