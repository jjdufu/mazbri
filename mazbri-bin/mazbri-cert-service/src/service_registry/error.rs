use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_core::{CoreError, ErrorResponse, error_codes};
use thiserror::Error;
#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("服务不存在: {0}")]
    ServiceNotFound(String),
    #[error("服务已存在: {0}")]
    ServiceAlreadyExists(String),
    #[error("服务名称不能为空")]
    EmptyServiceName,
    #[error("显示名称不能为空")]
    EmptyDisplayName,
}
pub type ServiceResult<T> = Result<T, ServiceError>;
impl From<sqlx::Error> for ServiceError {
    fn from(e: sqlx::Error) -> Self {
        Self::Core(CoreError::from(e))
    }
}
impl ErrorResponse for ServiceError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Core(e) => e.status_code(),
            Self::ServiceNotFound(_) => StatusCode::NOT_FOUND,
            Self::ServiceAlreadyExists(_) => StatusCode::CONFLICT,
            Self::EmptyServiceName | Self::EmptyDisplayName => StatusCode::BAD_REQUEST,
        }
    }
    fn error_code(&self) -> &'static str {
        match self {
            Self::Core(e) => e.error_code(),
            Self::ServiceNotFound(_) => error_codes::NOT_FOUND,
            Self::ServiceAlreadyExists(_) => "SERVICE_ALREADY_EXISTS",
            Self::EmptyServiceName | Self::EmptyDisplayName => error_codes::VALIDATION_ERROR,
        }
    }
    fn detail(&self) -> Option<String> {
        match self {
            Self::Core(e) => e.detail(),
            _ => Some(self.to_string()),
        }
    }
    fn safe_message(&self) -> String {
        match self {
            Self::Core(e) => e.safe_message(),
            _ => self.detail().unwrap_or_else(|| self.title()),
        }
    }
}
impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        ErrorResponse::into_response(self)
    }
}
