use super::ValidateTomlRequest;
use crate::error::ApiError;
use crate::sanitize::sanitize_json_value;
use crate::services::config_center as service;
use axum::Json;
pub async fn validate_toml_endpoint(
    Json(payload): Json<ValidateTomlRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(sanitize_json_value(service::validate_toml_endpoint(
        &payload,
    )?)))
}
