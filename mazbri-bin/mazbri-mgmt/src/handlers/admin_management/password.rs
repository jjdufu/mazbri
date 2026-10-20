use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::state::AppState;
use crate::handlers::types::{ForcePasswordResetRequest, ResetPasswordRequest};
use crate::middleware::auth::AdminClaims;
use crate::response::ApiResponse;
use crate::services::admin_management as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, State},
    response::IntoResponse,
};
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn reset_admin_password(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<ResetPasswordRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data = svc::reset_password(
        &state.admin_db,
        &state.audit,
        &state.password_policy_cache,
        &claims,
        id,
        &req,
        client_ip,
    )
    .await?;
    Ok(ApiResponse::ok(data))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn force_password_reset(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<ForcePasswordResetRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data =
        svc::force_password_reset(&state.admin_db, &state.audit, &claims, id, &req, client_ip)
            .await?;
    Ok(ApiResponse::ok(data))
}
