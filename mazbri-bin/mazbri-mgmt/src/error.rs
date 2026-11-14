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
    /// 平台核心内置配置（`global` 及核心微服务）受保护不可删除。
    #[error("平台核心配置受保护不可删除，请使用编辑配置或历史版本回滚")]
    CoreServiceConfigProtected,
    /// 自定义扩展服务仍存在活跃心跳运行节点，禁止删除生效配置。
    #[error("该服务仍存在在线运行实例，禁止删除生效配置")]
    ConfigInUse,
    /// 发布草稿时通道编码已归属其他供应商，禁止跨供应商覆盖。
    #[error("通道编码 '{0}' 已归属其他供应商，禁止覆盖")]
    ChannelCodeOwnedByOtherSupplier(String),
    /// 发布草稿时路由优先级或同产品活跃路由与其他规则冲突。
    #[error("{0}")]
    RouteConflict(String),
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
            ApiError::CoreServiceConfigProtected
            | ApiError::ConfigInUse
            | ApiError::ChannelCodeOwnedByOtherSupplier(_)
            | ApiError::RouteConflict(_) => StatusCode::CONFLICT,
        }
    }
    fn error_code(&self) -> &'static str {
        match self {
            ApiError::Core(e) => e.error_code(),
            ApiError::Unauthorized => error_codes::AUTH_ERROR,
            ApiError::Forbidden => error_codes::PERMISSION_DENIED,
            ApiError::LicenseIssuanceNotAllowed => "license_issuance_not_allowed",
            ApiError::SuperAdminProtected => "super_admin_protected",
            ApiError::CoreServiceConfigProtected => "CORE_SERVICE_CONFIG_PROTECTED",
            ApiError::ConfigInUse => "CONFIG_IN_USE",
            ApiError::ChannelCodeOwnedByOtherSupplier(_) => "CHANNEL_CODE_OWNED_BY_OTHER_SUPPLIER",
            ApiError::RouteConflict(_) => "ROUTE_CONFLICT",
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
            ApiError::CoreServiceConfigProtected => {
                Some("平台核心配置受保护不可删除，请使用编辑配置或历史版本回滚".to_string())
            }
            ApiError::ConfigInUse => {
                Some("该服务仍存在在线运行实例，禁止删除生效配置".to_string())
            }
            ApiError::ChannelCodeOwnedByOtherSupplier(code) => Some(format!(
                "通道编码 '{code}' 已归属其他供应商，请更换通道编码后再发布"
            )),
            ApiError::RouteConflict(msg) => Some(msg.clone()),
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
