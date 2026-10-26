use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_core::{CoreError, ErrorResponse, error_codes};
use thiserror::Error;
#[derive(Debug, Error)]
pub enum ApiError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Permission denied")]
    Forbidden,
    #[error("授权未生效，请先导入或续期许可证")]
    LicenseIssuanceNotAllowed,
    /// 目标是超级管理员：不允许经由账号管理接口重置密码 / MFA、改角色或停用。
    #[error("超级管理员账号受保护")]
    SuperAdminProtected,
}
impl ApiError {
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Core(CoreError::Internal(msg.into()))
    }
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Core(CoreError::Validation(msg.into()))
    }
    #[must_use]
    pub fn not_found(resource: String, id: String) -> Self {
        Self::Core(CoreError::NotFound { resource, id })
    }
    #[must_use]
    pub fn license_issuance_not_allowed() -> Self {
        Self::LicenseIssuanceNotAllowed
    }
}
impl ErrorResponse for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::Core(e) => e.status_code(),
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden
            | ApiError::LicenseIssuanceNotAllowed
            | ApiError::SuperAdminProtected => StatusCode::FORBIDDEN,
        }
    }
    fn error_code(&self) -> &'static str {
        match self {
            ApiError::Core(e) => e.error_code(),
            ApiError::Unauthorized => error_codes::AUTH_ERROR,
            ApiError::Forbidden => error_codes::PERMISSION_DENIED,
            ApiError::LicenseIssuanceNotAllowed => "license_issuance_not_allowed",
            ApiError::SuperAdminProtected => "super_admin_protected",
        }
    }
    fn detail(&self) -> Option<String> {
        match self {
            ApiError::Core(e) => e.detail(),
            ApiError::LicenseIssuanceNotAllowed => {
                Some("授权未生效，请先导入或续期许可证".to_string())
            }
            ApiError::SuperAdminProtected => Some(
                "超级管理员账号受保护：不能通过账号管理重置其密码或 MFA、修改角色或停用"
                    .to_string(),
            ),
            _ => None,
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = self.error_code();
        let message = self.safe_message();
        let mut response = ErrorResponse::into_response(self);
        crate::audit::AuditErrorInfo::attach(&mut response, code, &message);
        response
    }
}
