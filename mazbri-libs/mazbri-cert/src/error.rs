use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_core::{ErrorResponse, core_error_response_match};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CertError {
    #[error(transparent)]
    Core(#[from] mazbri_core::CoreError),
    #[error("证书未找到: {0}")]
    NotFound(String),
    #[error("证书主体不在规范服务列表: {0}")]
    SubjectNotCanonical(String),
    #[error("服务 {0} 不需要 mTLS 证书（仅 HTTP + Bearer token）")]
    SubjectMtlsNotRequired(String),
    #[error("SAN 配置错误: {0}")]
    SanConfig(String),
    #[error("证书已作废: {0}")]
    AlreadyRevoked(String),
    #[error("无效的主体: {0}")]
    InvalidSubject(String),
    #[error("无效的请求: {0}")]
    InvalidRequest(String),
    #[error("证书生成失败: {0}")]
    GenerationFailed(String),
    #[error("证书验证失败: {0}")]
    VerificationFailed(String),
    #[error("CA 初始化失败: {0}")]
    CaInitFailed(String),
    #[error("认证失败: {0}")]
    Unauthorized(String),
    #[error("密钥加密/解密失败: {0}")]
    KeyEncryption(String),
    #[error("基础授权包未导入，请联系管理员添加授权")]
    BaseAuthorizationNotActive,
    #[error("基础授权包已过期，请先完成续期")]
    BaseAuthorizationExpired,
    #[error("根保护包已过期，请先完成续期")]
    RootPackageExpired,
    #[error("{message}")]
    License { code: &'static str, message: String },
}

pub type CertResult<T> = Result<T, CertError>;

impl ErrorResponse for CertError {
    fn status_code(&self) -> StatusCode {
        core_error_response_match!(self, status_code, {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::AlreadyRevoked(_)
            | Self::InvalidSubject(_)
            | Self::InvalidRequest(_)
            | Self::SubjectNotCanonical(_)
            | Self::SubjectMtlsNotRequired(_)
            | Self::VerificationFailed(_) => StatusCode::BAD_REQUEST,
            Self::BaseAuthorizationNotActive
            | Self::BaseAuthorizationExpired
            | Self::RootPackageExpired => StatusCode::FORBIDDEN,
            Self::License { .. } => StatusCode::BAD_REQUEST,
            Self::GenerationFailed(_)
            | Self::CaInitFailed(_)
            | Self::KeyEncryption(_)
            | Self::SanConfig(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })
    }
    fn error_code(&self) -> &'static str {
        core_error_response_match!(self, error_code, {
            Self::NotFound(_) => "CERTIFICATE_NOT_FOUND",
            Self::SubjectNotCanonical(_) => "SUBJECT_NOT_CANONICAL",
            Self::SubjectMtlsNotRequired(_) => "SUBJECT_MTLS_NOT_REQUIRED",
            Self::SanConfig(_) => "SAN_CONFIG_ERROR",
            Self::AlreadyRevoked(_) => "CERTIFICATE_ALREADY_REVOKED",
            Self::InvalidSubject(_) => "INVALID_SUBJECT",
            Self::InvalidRequest(_) => "INVALID_REQUEST",
            Self::GenerationFailed(_) => "GENERATION_FAILED",
            Self::VerificationFailed(_) => "VERIFICATION_FAILED",
            Self::CaInitFailed(_) => "CA_INIT_FAILED",
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::KeyEncryption(_) => "KEY_ENCRYPTION_ERROR",
            Self::BaseAuthorizationNotActive => "base_authorization_not_active",
            Self::BaseAuthorizationExpired => "base_authorization_expired",
            Self::RootPackageExpired => "root_package_expired",
            Self::License { code, .. } => code,
        })
    }
    fn details(&self) -> Option<serde_json::Value> {
        core_error_response_match!(self, details, {
            Self::NotFound(id) => Some(json!({ "certificate_id" : id })),
            Self::SubjectNotCanonical(reason) => Some(json!({
                "reason" : reason,
                "allowed" : mazbri_core::ServiceName::mtls_names(),
            })),
            Self::SubjectMtlsNotRequired(name) => Some(json!({
                "subject" : name,
                "allowed" : mazbri_core::ServiceName::mtls_names(),
            })),
            _ => None,
        })
    }
    fn detail(&self) -> Option<String> {
        core_error_response_match!(self, detail, {
            Self::BaseAuthorizationNotActive => Some("基础授权包未导入，请联系管理员添加授权".to_string()),
            Self::BaseAuthorizationExpired => Some("基础授权包已过期，请先完成续期".to_string()),
            Self::RootPackageExpired => Some("根保护包已过期，请先完成续期".to_string()),
            Self::License { message, .. } => Some(message.clone()),
            _ => Some(self.to_string()),
        })
    }
    fn title(&self) -> String {
        core_error_response_match!(self, title, {
            Self::BaseAuthorizationNotActive => "基础授权包未导入，请联系管理员添加授权".to_string(),
            Self::BaseAuthorizationExpired => "基础授权包已过期，请先完成续期".to_string(),
            Self::RootPackageExpired => "根保护包已过期，请先完成续期".to_string(),
            Self::License { message, .. } => message.clone(),
            _ => self.to_string(),
        })
    }
}

impl IntoResponse for CertError {
    fn into_response(self) -> Response {
        ErrorResponse::into_response(self)
    }
}

impl From<sqlx::Error> for CertError {
    fn from(e: sqlx::Error) -> Self {
        Self::Core(mazbri_core::CoreError::from(e))
    }
}
