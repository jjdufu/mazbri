use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::state::AppState;
use crate::middleware::auth::AdminClaims;
use crate::response::ApiResponse;
use crate::services::admin_management as svc;
use axum::{
    Extension,
    extract::{ConnectInfo, Path, State},
    response::IntoResponse,
};
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn reset_admin_mfa(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Result<axum::response::Response, ApiError> {
    if let Err(error) = svc::precheck_mfa_reset(&state.admin_db, &claims, id).await {
        return Ok(error.into_response());
    }
    if let Err(response) = crate::handlers::mfa_guard::require_category(
        &state,
        &claims,
        crate::services::mfa::Category::Admin,
    )
    .await
    {
        return Ok(*response);
    }
    let client_ip = extract_client_ip(&headers, Some(addr));
    let data = svc::reset_mfa(&state.admin_db, &state.audit, &claims, id, client_ip).await?;
    Ok(ApiResponse::ok(data).into_response())
}
