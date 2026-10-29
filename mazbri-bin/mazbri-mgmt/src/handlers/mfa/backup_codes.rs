use super::super::AppState;
use crate::audit::extract_client_ip;
use crate::middleware::auth::extract_user_id_response;
use crate::services::mfa as svc;
use crate::services::mfa::Category;
use axum::{
    Extension,
    extract::{ConnectInfo, State},
    response::IntoResponse,
};
pub async fn regenerate_backup_codes(
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
    match crate::repositories::mfa::is_mfa_enabled(&state.admin_db, user_id).await {
        Ok(Some(true)) => {}
        Ok(_) => {
            return crate::response::ApiResponse::err(
                axum::http::StatusCode::NOT_FOUND,
                mazbri_i18n::mfa::user_not_found(),
            );
        }
        Err(error) => {
            tracing::error!("failed to read MFA state before regenerating codes: {error}");
            return crate::response::ApiResponse::err(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::database_error(),
            );
        }
    }
    if let Err(response) =
        crate::handlers::mfa_guard::require_category(&state, &claims, Category::Mfa).await
    {
        return *response;
    }
    svc::regenerate_backup_codes(
        &state.admin_db,
        &state.mfa_cache,
        &state.audit,
        user_id,
        &claims.sub,
        client_ip,
    )
    .await
}
