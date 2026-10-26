use super::super::AppState;
use crate::audit::extract_client_ip;
use crate::services::mfa as svc;
use axum::{
    Json,
    extract::{ConnectInfo, State},
};
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MfaRequiredSetupRequest {
    pub temp_token: String,
}
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MfaRequiredEnableRequest {
    pub temp_token: String,
    pub totp_code: String,
}
pub async fn mfa_required_setup(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(req): Json<MfaRequiredSetupRequest>,
) -> axum::response::Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    svc::required_setup(svc::RequiredSetupParams {
        pool: &state.admin_db,
        redis: &state.redis,
        mfa_cache: &state.mfa_cache,
        audit: &state.audit,
        jwt_secret: &state.auth_state.jwt_secret,
        temp_token: &req.temp_token,
        client_ip,
    })
    .await
}
pub async fn mfa_required_enable(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(req): Json<MfaRequiredEnableRequest>,
) -> axum::response::Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    svc::required_enable(svc::RequiredEnableParams {
        pool: &state.admin_db,
        redis: &state.redis,
        mfa_cache: &state.mfa_cache,
        audit: &state.audit,
        jwt_secret: &state.auth_state.jwt_secret,
        jwt_expiration_secs: state.config.auth.jwt_expiration_secs,
        temp_token: &req.temp_token,
        totp_code: &req.totp_code,
        client_ip,
    })
    .await
}
