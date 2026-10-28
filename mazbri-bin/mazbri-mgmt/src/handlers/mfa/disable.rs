use super::super::AppState;
use crate::audit::extract_client_ip;
use crate::middleware::auth::{REFRESH_COOKIE, cookie_value, extract_user_id_response};
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
    let refresh_token = cookie_value(&headers, REFRESH_COOKIE);
    let rotation = match svc::guard_session_rotation(
        &claims,
        refresh_token.as_deref(),
        &state.auth_state.jwt_secret,
        chrono::Utc::now().timestamp(),
    ) {
        Ok(rotation) => rotation,
        Err(response) => return *response,
    };
    if let Err(response) =
        crate::handlers::mfa_guard::require_fresh_mfa(&state, &claims, FreshPurpose::MfaDisable)
            .await
    {
        return *response;
    }
    svc::disable(svc::DisableParams {
        pool: &state.admin_db,
        audit: &state.audit,
        user_id,
        claims: &claims,
        old_refresh: &rotation.old_refresh,
        sxp: rotation.sxp,
        jwt_secret: &state.auth_state.jwt_secret,
        redis: &state.redis,
        access_ttl_fallback: state.config.auth.jwt_expiration_secs,
        client_ip,
    })
    .await
}
