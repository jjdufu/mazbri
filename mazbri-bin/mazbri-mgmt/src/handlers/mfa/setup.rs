use super::super::AppState;
use super::super::types::MfaSetupRequest;
use crate::audit::extract_client_ip;
use crate::middleware::auth::extract_user_id_response;
use crate::services::mfa as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, State},
    response::IntoResponse,
};
pub async fn mfa_setup(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
    headers: axum::http::HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(req): Json<MfaSetupRequest>,
) -> axum::response::Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let user_id = match extract_user_id_response(&claims) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };
    svc::setup(svc::SetupParams {
        pool: &state.admin_db,
        password_policy_cache: &state.password_policy_cache,
        mfa_cache: &state.mfa_cache,
        audit: &state.audit,
        user_id,
        username: &claims.sub,
        current_password: &req.current_password,
        client_ip,
    })
    .await
}
