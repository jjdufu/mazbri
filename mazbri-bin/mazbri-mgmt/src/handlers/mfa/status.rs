use super::super::AppState;
use crate::middleware::auth::extract_user_id_response;
use crate::services::mfa as svc;
use axum::{Extension, extract::State, response::IntoResponse};
pub async fn mfa_status(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::middleware::auth::AdminClaims>,
) -> axum::response::Response {
    let user_id = match extract_user_id_response(&claims) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };
    svc::status(
        &state.admin_db,
        &state.mfa_cache,
        state.config.mfa.digits,
        user_id,
    )
    .await
}
