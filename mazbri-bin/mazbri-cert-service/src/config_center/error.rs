use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_core::ErrorResponse;
use serde::Serialize;
use thiserror::Error;
#[derive(Debug, Clone, Serialize)]
pub struct TomlValidationError {
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub raw_error: String,
}
impl std::fmt::Display for TomlValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(col)) => {
                write!(f, "行 {}, 列 {}: {}", line, col, self.message)
            }
            (Some(line), None) => write!(f, "行 {}: {}", line, self.message),
            _ => write!(f, "{}", self.message),
        }
    }
}
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Core(#[from] mazbri_core::CoreError),
    #[error("配置不存在: {service_name}")]
    NotFound { service_name: String },
    #[error("TOML 验证失败: {details}")]
    TomlValidation { details: TomlValidationError },
    #[error("配置已存在: {service_name}")]
    AlreadyExists { service_name: String },
    #[error("验证错误: {0}")]
    Validation(String),
    #[error("加密错误: {0}")]
    Encryption(String),
}
pub type ConfigResult<T> = Result<T, ConfigError>;
impl ErrorResponse for ConfigError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Core(e) => e.status_code(),
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::TomlValidation { .. } | Self::AlreadyExists { .. } | Self::Validation(_) => {
                StatusCode::BAD_REQUEST
            }
            Self::Encryption(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    fn error_code(&self) -> &'static str {
        match self {
            Self::Core(e) => e.error_code(),
            Self::NotFound { .. } => "CONFIG_NOT_FOUND",
            Self::TomlValidation { .. } => "TOML_VALIDATION_ERROR",
            Self::AlreadyExists { .. } => "CONFIG_ALREADY_EXISTS",
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::Encryption(_) => "ENCRYPTION_ERROR",
        }
    }
    fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::Core(e) => e.details(),
            Self::NotFound { service_name } => {
                Some(serde_json::json!({ "service_name" : service_name }))
            }
            Self::TomlValidation { details } => Some(serde_json::json!(
                { "message" : details.message, "line" : details.line, "column" :
                details.column }
            )),
            Self::AlreadyExists { service_name } => {
                Some(serde_json::json!({ "service_name" : service_name }))
            }
            Self::Validation(msg) => Some(serde_json::json!({ "message" : msg })),
            Self::Encryption(_) => Some(serde_json::json!({ "type" : "encryption_error" })),
        }
    }
    fn detail(&self) -> Option<String> {
        match self {
            Self::Core(e) => e.detail(),
            _ => Some(self.to_string()),
        }
    }
    fn title(&self) -> String {
        match self {
            Self::Core(e) => e.title(),
            _ => self.to_string(),
        }
    }
}
impl IntoResponse for ConfigError {
    fn into_response(self) -> Response {
        ErrorResponse::into_response(self)
    }
}
impl From<sqlx::Error> for ConfigError {
    fn from(e: sqlx::Error) -> Self {
        Self::Core(mazbri_core::CoreError::from(e))
    }
}
