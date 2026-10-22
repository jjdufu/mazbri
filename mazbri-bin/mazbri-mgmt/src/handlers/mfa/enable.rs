use super::super::AppState;
use super::super::types::MfaEnableRequest;
use crate::audit::extract_client_ip;
use crate::middleware::auth::extract_user_id_response;
use crate::services::mfa as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, State},
    response::IntoResponse,
};
pub async fn mfa_enable(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
    headers: axum::http::HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(req): Json<MfaEnableRequest>,
) -> axum::response::Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let user_id = match extract_user_id_response(&claims) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };
    svc::enable(
        &state.admin_db,
        &state.mfa_cache,
        &state.audit,
        user_id,
        &claims.sub,
        &req.totp_code,
        client_ip,
    )
    .await
}
