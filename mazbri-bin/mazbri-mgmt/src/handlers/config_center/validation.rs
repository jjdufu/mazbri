use super::ValidateTomlRequest;
use crate::error::ApiError;
use crate::services::config_center as service;
use axum::Json;
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn validate_toml_endpoint(
    Json(payload): Json<ValidateTomlRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(service::validate_toml_endpoint(&payload)?))
}
