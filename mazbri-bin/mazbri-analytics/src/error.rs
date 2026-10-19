use crate::greptimedb::SanitizationError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_core::{CoreError, ErrorResponse, core_error_response_match, error_codes};
use thiserror::Error;
#[derive(Debug, Error)]
pub enum AnalyticsError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("GreptimeDB 错误: {0}")]
    GreptimeDB(String),
    #[error("ClickHouse 错误: {0}")]
    ClickHouse(String),
    #[error("配置错误: {0}")]
    Config(String),
    #[error("HTTP 请求失败: {0}")]
    Http(#[from] reqwest::Error),
}
pub type Result<T> = std::result::Result<T, AnalyticsError>;
impl ErrorResponse for AnalyticsError {
    fn status_code(&self) -> StatusCode {
        core_error_response_match!(self, status_code, {
            Self::GreptimeDB(_) | Self::ClickHouse(_) | Self::Config(_) | Self::Http(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })
    }
    fn error_code(&self) -> &'static str {
        core_error_response_match!(self, error_code, {
            Self::GreptimeDB(_) | Self::ClickHouse(_) => error_codes::DATABASE_ERROR,
            Self::Config(_) => error_codes::CONFIG_ERROR,
            Self::Http(_) => error_codes::NETWORK_ERROR,
        })
    }
    fn detail(&self) -> Option<String> {
        core_error_response_match!(self, detail, {
            _ => Some(self.to_string()),
        })
    }
    fn safe_message(&self) -> String {
        core_error_response_match!(self, safe_message, {
            Self::GreptimeDB(_) | Self::ClickHouse(_) => "数据库查询失败".to_string(),
            Self::Config(_) => "配置错误".to_string(),
            Self::Http(_) => "网络请求失败".to_string(),
        })
    }
}
impl IntoResponse for AnalyticsError {
    fn into_response(self) -> Response {
        ErrorResponse::into_response(self)
    }
}
impl From<SanitizationError> for AnalyticsError {
    fn from(e: SanitizationError) -> Self {
        Self::GreptimeDB(e.0)
    }
}
