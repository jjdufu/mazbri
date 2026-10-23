use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_core::{CoreError, ErrorResponse, error_codes};
use thiserror::Error;
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("配置无效: {0}")]
    ConfigInvalid(String),
    #[error("配置验证失败: {0}")]
    ConfigValidationFailed(String),
    #[error("配置应用失败: {0}")]
    ConfigApplyFailed(String),
}
pub type RegistryResult<T> = std::result::Result<T, RegistryError>;
impl ErrorResponse for RegistryError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Core(e) => e.status_code(),
            Self::ConfigInvalid(_) | Self::ConfigValidationFailed(_) => StatusCode::BAD_REQUEST,
            Self::ConfigApplyFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    fn error_code(&self) -> &'static str {
        match self {
            Self::Core(e) => e.error_code(),
            Self::ConfigInvalid(_) | Self::ConfigValidationFailed(_) => {
                error_codes::VALIDATION_ERROR
            }
            Self::ConfigApplyFailed(_) => error_codes::CONFIG_ERROR,
        }
    }
    fn detail(&self) -> Option<String> {
        match self {
            Self::Core(e) => e.detail(),
            _ => Some(self.to_string()),
        }
    }
}
impl IntoResponse for RegistryError {
    fn into_response(self) -> Response {
        ErrorResponse::into_response(self)
    }
}
