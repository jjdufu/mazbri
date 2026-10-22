use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use mazbri_core::{CoreError, ErrorResponse, error_codes};
use serde_json::json;
#[derive(Debug, thiserror::Error)]
pub enum ConfigClientError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("无效的配置中心地址: {0}")]
    InvalidAddress(#[source] std::net::AddrParseError),
    #[error("配置未找到: {service_name} - {message}")]
    ConfigNotFound {
        service_name: String,
        message: String,
    },
}
impl ErrorResponse for ConfigClientError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Core(e) => e.status_code(),
            Self::InvalidAddress(_) => StatusCode::BAD_REQUEST,
            Self::ConfigNotFound { .. } => StatusCode::NOT_FOUND,
        }
    }
    fn error_code(&self) -> &'static str {
        match self {
            Self::Core(e) => e.error_code(),
            Self::InvalidAddress(_) => error_codes::VALIDATION_ERROR,
            Self::ConfigNotFound { .. } => error_codes::NOT_FOUND,
        }
    }
    fn detail(&self) -> Option<String> {
        match self {
            Self::Core(e) => e.detail(),
            Self::ConfigNotFound {
                service_name,
                message,
            } => Some(format!(
                "{}: {} - {}",
                self.error_code(),
                service_name,
                message
            )),
            Self::InvalidAddress(e) => Some(e.to_string()),
        }
    }
    fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::Core(e) => e.details(),
            Self::ConfigNotFound {
                service_name,
                message,
            } => Some(json!(
                { "service_name" : service_name, "message" : message }
            )),
            Self::InvalidAddress(_) => None,
        }
    }
}
impl IntoResponse for ConfigClientError {
    fn into_response(self) -> Response {
        ErrorResponse::into_response(self)
    }
}
