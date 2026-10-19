use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::state::AppState;
use crate::handlers::types::SetAdminStatusRequest;
use crate::middleware::auth::AdminClaims;
use crate::response::ApiResponse;
use crate::services::admin_management as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, State},
    response::IntoResponse,
};
pub async fn set_admin_status(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<SetAdminStatusRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data = svc::set_status(&state.admin_db, &state.audit, &claims, id, &req, client_ip).await?;
    Ok(ApiResponse::ok(data))
}
