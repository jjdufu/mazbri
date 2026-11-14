use super::super::AppState;
use super::super::types::MfaEnableRequest;
use crate::audit::extract_client_ip;
use crate::services::mfa as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, State},
};
pub async fn mfa_enable(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
    headers: axum::http::HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(req): Json<MfaEnableRequest>,
) -> axum::response::Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let refresh_token =
        crate::middleware::auth::cookie_value(&headers, crate::middleware::auth::REFRESH_COOKIE);
    svc::enable(svc::EnableParams {
        pool: &state.admin_db,
        redis: &state.redis,
        mfa_cache: &state.mfa_cache,
        audit: &state.audit,
        claims: &claims,
        jwt_secret: &state.auth_state.jwt_secret,
        access_ttl_fallback: state.config.auth.jwt_expiration_secs,
        totp_code: &req.totp_code,
        refresh_token: refresh_token.as_deref(),
        client_ip,
    })
    .await
}
