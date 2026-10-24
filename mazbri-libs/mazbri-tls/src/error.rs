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
    /// 目标主机名暂时无法解析（如 compose/k8s 服务尚未注册 DNS），可重试。
    #[error("地址解析失败: {0}")]
    Resolve(String),
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
            Self::Resolve(_) => StatusCode::SERVICE_UNAVAILABLE,
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
            Self::Resolve(_) => "TLS_RESOLVE_FAILED",
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

impl TlsError {
    /// 转成 `CoreError`，保留可重试语义：解析失败映射为 `CoreError::Network`。
    #[must_use]
    pub fn into_core(self) -> mazbri_core::CoreError {
        match self {
            Self::Core(e) => e,
            Self::Resolve(_) => mazbri_core::CoreError::Network(self.to_string()),
            Self::Config(_) => mazbri_core::CoreError::Config(self.to_string()),
            other => mazbri_core::CoreError::Tls(other.to_string()),
        }
    }
}

#[cfg(test)]
mod into_core_tests {
    use super::*;
    use mazbri_core::CoreError;

    #[test]
    fn resolve_maps_to_network() {
        assert!(matches!(
            TlsError::Resolve("x:1".into()).into_core(),
            CoreError::Network(_)
        ));
        assert!(matches!(
            TlsError::Config("bad".into()).into_core(),
            CoreError::Config(_)
        ));
        assert!(matches!(
            TlsError::HandshakeFailed("h".into()).into_core(),
            CoreError::Tls(_)
        ));
    }
}
