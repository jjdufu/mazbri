use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_core::{ErrorResponse, core_error_response_match};
use serde_json::json;
use thiserror::Error;
#[derive(Debug, Error)]
pub enum TlsError {
    #[error(transparent)]
    Core(#[from] mazbri_core::CoreError),
    #[error("TLS 配置错误: {0}")]
    Config(String),
    #[error("主体证书未找到: {0}")]
    CertificateNotFound(String),
    #[error("证书已过期: {0}")]
    CertificateExpired(String),
    #[error("证书已作废")]
    CertificateRevoked,
    #[error("发放证书失败: {0}")]
    IssueFailed(String),
    #[error("续期证书失败: {0}")]
    RenewFailed(String),
    #[error("TLS 握手失败: {0}")]
    HandshakeFailed(String),
    #[error("无效的证书格式: {0}")]
    InvalidCertificate(String),
}
pub type TlsResult<T> = Result<T, TlsError>;
impl ErrorResponse for TlsError {
    fn status_code(&self) -> StatusCode {
        core_error_response_match!(self, status_code, {
            Self::Config(_) | Self::InvalidCertificate(_) => StatusCode::BAD_REQUEST,
            Self::CertificateNotFound(_) => StatusCode::NOT_FOUND,
            Self::CertificateExpired(_) | Self::CertificateRevoked => StatusCode::GONE,
            Self::IssueFailed(_) | Self::RenewFailed(_) | Self::HandshakeFailed(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })
    }
    fn error_code(&self) -> &'static str {
        core_error_response_match!(self, error_code, {
            Self::Config(_) => "TLS_CONFIG_ERROR",
            Self::CertificateNotFound(_) => "TLS_CERTIFICATE_NOT_FOUND",
            Self::CertificateExpired(_) => "TLS_CERTIFICATE_EXPIRED",
            Self::CertificateRevoked => "TLS_CERTIFICATE_REVOKED",
            Self::IssueFailed(_) => "TLS_ISSUE_FAILED",
            Self::RenewFailed(_) => "TLS_RENEW_FAILED",
            Self::HandshakeFailed(_) => "TLS_HANDSHAKE_FAILED",
            Self::InvalidCertificate(_) => "TLS_INVALID_CERTIFICATE",
        })
    }
    fn details(&self) -> Option<serde_json::Value> {
        core_error_response_match!(self, details, {
            Self::CertificateExpired(date) => Some(json!({ "expired_at" : date })),
            Self::CertificateNotFound(subject) => Some(json!({ "subject" : subject })),
            _ => None,
        })
    }
    fn detail(&self) -> Option<String> {
        core_error_response_match!(self, detail, {
            _ => Some(self.to_string()),
        })
    }
    fn title(&self) -> String {
        core_error_response_match!(self, title, {
            _ => self.to_string(),
        })
    }
}
impl IntoResponse for TlsError {
    fn into_response(self) -> Response {
        ErrorResponse::into_response(self)
    }
}
impl From<reqwest::Error> for TlsError {
    fn from(e: reqwest::Error) -> Self {
        Self::Core(mazbri_core::CoreError::Network(e.to_string()))
    }
}
impl From<std::io::Error> for TlsError {
    fn from(e: std::io::Error) -> Self {
        Self::Core(mazbri_core::CoreError::from(e))
    }
}
