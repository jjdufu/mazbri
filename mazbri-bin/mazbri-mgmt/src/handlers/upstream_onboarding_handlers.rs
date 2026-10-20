use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::middleware::auth::{AdminClaims, extract_user_id, is_operator_or_above};
use crate::response::ApiResponse;
use crate::services::upstream_onboarding as svc;
use axum::response::Response;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, State},
    http::HeaderMap,
    response::IntoResponse,
};
use serde::Deserialize;

/// POST /api/v1/mgmt/upstreams/onboarding
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn onboard_upstream(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<svc::UpstreamOnboardingRequest>,
) -> Result<Response, ApiError> {
    if !is_operator_or_above(&claims) {
        return Err(ApiError::Forbidden);
    }
    let data = svc::onboard(
        &state.db,
        &state.audit,
        &payload,
        extract_user_id(&claims)?,
        &claims.sub,
        extract_client_ip(&headers, Some(addr)),
    )
    .await?;
    if data.dry_run {
        return Ok(ApiResponse::ok(data).into_response());
    }
    Ok(crate::response::created(data))
}

/// POST /api/v1/mgmt/upstreams/onboarding/dry-run
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn onboard_upstream_dry_run(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Json(payload): Json<svc::UpstreamOnboardingRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_operator_or_above(&claims) {
        return Err(ApiError::Forbidden);
    }
    let data = svc::dry_run(&state.db, &payload).await?;
    Ok(ApiResponse::ok(data))
}

#[derive(Debug, Deserialize)]
pub struct ValidateConfigBody {
    pub config: serde_json::Value,
    pub upstream_url: String,
}

/// POST /api/v1/mgmt/suppliers/{id}/validate-config
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn validate_supplier_config(
    State(_state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<String>,
    Json(body): Json<ValidateConfigBody>,
) -> Result<impl IntoResponse, ApiError> {
    if !is_operator_or_above(&claims) {
        return Err(ApiError::Forbidden);
    }
    let _supplier_id = uuid::Uuid::parse_str(&id)
        .map_err(|e| ApiError::validation(format!("供应商 ID 无效: {e}")))?;
    let validated = svc::validate_config_payload(&body.config, &body.upstream_url).await?;
    Ok(ApiResponse::ok(serde_json::json!({
        "valid": true,
        "config": validated,
    })))
}
