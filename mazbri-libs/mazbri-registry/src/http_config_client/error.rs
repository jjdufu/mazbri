use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use mazbri_core::{CoreError, ErrorResponse, error_codes};
use serde_json::json;
#[derive(Debug, thiserror::Error)]
pub enum HttpConfigClientError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("HTTP request failed: {0}")]
    RequestFailed(String),
    #[error("Config not found: {service_name} - {message}")]
    ConfigNotFound {
        service_name: String,
        message: String,
    },
    #[error("Node registration failed: {0}")]
    RegistrationFailed(String),
    #[error("Response parsing failed: {0}")]
    ResponseParseFailed(String),
}
impl ErrorResponse for HttpConfigClientError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Core(e) => e.status_code(),
            Self::RequestFailed(_) | Self::RegistrationFailed(_) | Self::ResponseParseFailed(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::ConfigNotFound { .. } => StatusCode::NOT_FOUND,
        }
    }
    fn error_code(&self) -> &'static str {
        match self {
            Self::Core(e) => e.error_code(),
            Self::RequestFailed(_) => error_codes::NETWORK_ERROR,
            Self::ConfigNotFound { .. } => error_codes::NOT_FOUND,
            Self::RegistrationFailed(_) => error_codes::INTERNAL_ERROR,
            Self::ResponseParseFailed(_) => error_codes::VALIDATION_ERROR,
        }
    }
    fn detail(&self) -> Option<String> {
        match self {
            Self::Core(e) => e.detail(),
            Self::ConfigNotFound {
                service_name,
                message,
            } => Some(format!("{service_name}: {message}")),
            other => Some(other.to_string()),
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
            _ => None,
        }
    }
}
impl IntoResponse for HttpConfigClientError {
    fn into_response(self) -> Response {
        ErrorResponse::into_response(self)
    }
}
