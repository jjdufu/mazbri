use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_core::{ErrorResponse, core_error_response_match, error_codes};
use serde_json::json;
use thiserror::Error;
#[derive(Debug, Error)]
pub enum AuthError {
    #[error(transparent)]
    Core(#[from] mazbri_core::CoreError),
    #[error("Invalid API key format: {0}")]
    InvalidApiKeyFormat(String),
    #[error("API key not found: {0}")]
    ApiKeyNotFound(String),
    #[error("API key expired: {0}")]
    ApiKeyExpired(String),
    #[error("API key disabled: {0}")]
    ApiKeyDisabled(String),
    #[error("Invalid JWT token: {0}")]
    InvalidJwtToken(String),
    #[error("JWT token expired: {0}")]
    JwtTokenExpired(String),
    #[error("IP not whitelisted: {0}")]
    IpNotWhitelisted(String),
    #[error("Tenant not found: {0}")]
    TenantNotFound(String),
    #[error("Tenant suspended: {0}")]
    TenantSuspended(String),
}
pub type AuthResult<T> = Result<T, AuthError>;
impl ErrorResponse for AuthError {
    fn status_code(&self) -> StatusCode {
        core_error_response_match!(self, status_code, {
            Self::InvalidApiKeyFormat(_)
            | Self::ApiKeyNotFound(_)
            | Self::ApiKeyExpired(_)
            | Self::InvalidJwtToken(_)
            | Self::JwtTokenExpired(_) => StatusCode::UNAUTHORIZED,
            Self::ApiKeyDisabled(_) | Self::IpNotWhitelisted(_) | Self::TenantSuspended(_) => {
                StatusCode::FORBIDDEN
            }
            Self::TenantNotFound(_) => StatusCode::NOT_FOUND,
        })
    }
    fn error_code(&self) -> &'static str {
        core_error_response_match!(self, error_code, {
            Self::InvalidApiKeyFormat(_) => "INVALID_API_KEY_FORMAT",
            Self::ApiKeyNotFound(_) => "API_KEY_NOT_FOUND",
            Self::ApiKeyExpired(_) => error_codes::AUTH_ERROR,
            Self::ApiKeyDisabled(_) => "API_KEY_DISABLED",
            Self::InvalidJwtToken(_) => "INVALID_JWT_TOKEN",
            Self::JwtTokenExpired(_) => error_codes::AUTH_ERROR,
            Self::IpNotWhitelisted(_) => error_codes::PERMISSION_DENIED,
            Self::TenantNotFound(_) => error_codes::NOT_FOUND,
            Self::TenantSuspended(_) => error_codes::PERMISSION_DENIED,
        })
    }
    fn safe_message(&self) -> String {
        core_error_response_match!(self, safe_message, {
            Self::InvalidApiKeyFormat(_) => "无效的 API Key 格式".to_string(),
            Self::ApiKeyNotFound(_) => "API Key 未找到".to_string(),
            Self::ApiKeyExpired(_) => "API Key 已过期".to_string(),
            Self::ApiKeyDisabled(_) => "API Key 已禁用".to_string(),
            Self::InvalidJwtToken(_) => "无效的 JWT Token".to_string(),
            Self::JwtTokenExpired(_) => "JWT Token 已过期".to_string(),
            Self::IpNotWhitelisted(_) => "IP 未在白名单中".to_string(),
            Self::TenantNotFound(_) => "租户未找到".to_string(),
            Self::TenantSuspended(_) => "租户已停用".to_string(),
        })
    }
    fn details(&self) -> Option<serde_json::Value> {
        core_error_response_match!(self, details, {
            Self::ApiKeyExpired(expired_at) => Some(json!({ "expired_at" : expired_at })),
            Self::TenantNotFound(_) => None,
            Self::IpNotWhitelisted(_) => None,
            _ => None,
        })
    }
    fn detail(&self) -> Option<String> {
        core_error_response_match!(self, detail, {
            other => Some(other.to_string()),
        })
    }
}
impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        ErrorResponse::into_response(self)
    }
}
impl From<sqlx::Error> for AuthError {
    fn from(err: sqlx::Error) -> Self {
        AuthError::Core(mazbri_core::CoreError::from(err))
    }
}
