use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_core::{CoreError, ErrorResponse, error_codes};
use thiserror::Error;
#[derive(Debug, Error)]
pub enum ApiError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Permission denied")]
    Forbidden,
    #[error("授权未生效，请先导入或续期许可证")]
    LicenseIssuanceNotAllowed,
}
impl ApiError {
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Core(CoreError::Internal(msg.into()))
    }
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Core(CoreError::Validation(msg.into()))
    }
    pub fn not_found(resource: String, id: String) -> Self {
        Self::Core(CoreError::NotFound { resource, id })
    }
    pub fn license_issuance_not_allowed() -> Self {
        Self::LicenseIssuanceNotAllowed
    }
}
impl ErrorResponse for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::Core(e) => e.status_code(),
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::LicenseIssuanceNotAllowed => StatusCode::FORBIDDEN,
        }
    }
    fn error_code(&self) -> &'static str {
        match self {
            ApiError::Core(e) => e.error_code(),
            ApiError::Unauthorized => error_codes::AUTH_ERROR,
            ApiError::Forbidden => error_codes::PERMISSION_DENIED,
            ApiError::LicenseIssuanceNotAllowed => "license_issuance_not_allowed",
        }
    }
    fn detail(&self) -> Option<String> {
        match self {
            ApiError::Core(e) => e.detail(),
            ApiError::LicenseIssuanceNotAllowed => {
                Some("授权未生效，请先导入或续期许可证".to_string())
            }
            _ => None,
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        ErrorResponse::into_response(self)
    }
}
