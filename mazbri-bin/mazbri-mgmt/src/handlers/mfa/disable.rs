use super::super::AppState;
use crate::audit::extract_client_ip;
use crate::middleware::auth::extract_user_id_response;
use crate::services::mfa as svc;
use crate::services::mfa::step_up::FreshPurpose;
use axum::{
    Extension,
    extract::{ConnectInfo, State},
    response::IntoResponse,
};
pub async fn mfa_disable(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
    headers: axum::http::HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
) -> axum::response::Response {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let user_id = match extract_user_id_response(&claims) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };
    if let Err(response) = svc::ensure_mfa_can_be_disabled(&state.admin_db, user_id).await {
        return *response;
    }
    if let Err(response) =
        crate::handlers::mfa_guard::require_fresh_mfa(&state, &claims, FreshPurpose::MfaDisable)
            .await
    {
        return *response;
    }
    svc::disable(svc::DisableParams {
        pool: &state.admin_db,
        mfa_cache: &state.mfa_cache,
        audit: &state.audit,
        user_id,
        claims: &claims,
        jwt_secret: &state.auth_state.jwt_secret,
        redis: &state.redis,
        access_ttl_fallback: state.config.auth.jwt_expiration_secs,
        client_ip,
    })
    .await
}
