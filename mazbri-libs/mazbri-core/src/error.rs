use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use serde_json::json;
use std::error::Error;
#[derive(Debug, Serialize)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub title: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}
pub mod error_codes {
    pub const DATABASE_ERROR: &str = "DATABASE_ERROR";
    pub const CONFIG_ERROR: &str = "CONFIG_ERROR";
    pub const AUTH_ERROR: &str = "AUTH_ERROR";
    pub const PERMISSION_DENIED: &str = "PERMISSION_DENIED";
    pub const NOT_FOUND: &str = "NOT_FOUND";
    pub const VALIDATION_ERROR: &str = "VALIDATION_ERROR";
    pub const INSUFFICIENT_BALANCE: &str = "INSUFFICIENT_BALANCE";
    pub const RATE_LIMITED: &str = "RATE_LIMITED";
    pub const RATE_LIMIT_EXCEEDED: &str = "RATE_LIMIT_EXCEEDED";
    pub const QUOTA_EXCEEDED: &str = "QUOTA_EXCEEDED";
    pub const BILLING_ERROR: &str = "BILLING_ERROR";
    pub const CERTIFICATE_ERROR: &str = "CERTIFICATE_ERROR";
    pub const TLS_ERROR: &str = "TLS_ERROR";
    pub const NETWORK_ERROR: &str = "NETWORK_ERROR";
    pub const SERVICE_UNAVAILABLE: &str = "SERVICE_UNAVAILABLE";
    pub const CIRCUIT_BREAKER_OPEN: &str = "CIRCUIT_BREAKER_OPEN";
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
    pub const IO_ERROR: &str = "IO_ERROR";
}
pub trait ErrorResponse: Error + Send + Sync + Sized + 'static {
    fn status_code(&self) -> StatusCode;
    fn error_code(&self) -> &'static str;
    fn details(&self) -> Option<serde_json::Value> {
        None
    }
    fn to_json(&self) -> serde_json::Value {
        let mut body = json!(
            { "error" : self.error_code(), "message" : self.safe_message(), }
        );
        if let Some(details) = self.details() {
            body["details"] = details;
        }
        body
    }
    fn error_type(&self) -> &'static str {
        "application_error"
    }
    fn problem_type(&self) -> String {
        format!("https://mazbri.com/errors/{}", self.error_code())
    }
    fn title(&self) -> String {
        self.to_string()
    }
    fn detail(&self) -> Option<String> {
        None
    }
    fn instance(&self) -> Option<String> {
        None
    }
    fn to_problem_details(&self, trace_id: Option<&str>) -> ProblemDetails {
        ProblemDetails {
            problem_type: self.problem_type(),
            title: self.title(),
            status: self.status_code().as_u16(),
            detail: self.detail(),
            instance: self.instance(),
            trace_id: trace_id.map(std::string::ToString::to_string),
        }
    }
    fn into_response(self) -> Response {
        let status = self.status_code();
        let error_code = self.error_code();
        if error_code == error_codes::DATABASE_ERROR {
            tracing::error!(
                error_code, status_code = status.as_u16(), message = % self.to_string(),
                debug_details = ? self.debug_details(), "API 错误响应"
            );
        } else {
            tracing::warn!(
                error_code, status_code = status.as_u16(), message = % self.to_string(),
                debug_details = ? self.debug_details(), "API 错误响应"
            );
        }
        let mut body = json!({ "error" : error_code, "message" : self.safe_message(), });
        if let Some(details) = self.details() {
            body["details"] = details;
        }
        (status, Json(body)).into_response()
    }
    fn is_client_error(&self) -> bool {
        self.status_code().is_client_error()
    }
    fn safe_message(&self) -> String {
        self.detail().unwrap_or_else(|| self.title())
    }
    fn debug_details(&self) -> Option<serde_json::Value> {
        self.details()
    }
}
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("数据库错误: {0}")]
    Database(#[from] sqlx::Error),
    #[error("配置错误: {0}")]
    Config(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("网络错误: {0}")]
    Network(String),
    #[error("认证失败: {0}")]
    Auth(String),
    #[error("权限不足: {0}")]
    PermissionDenied(String),
    #[error("TLS 错误: {0}")]
    Tls(String),
    #[error("请求超限: {0}")]
    RateLimit(String),
    #[error("未找到: 资源 {resource}，ID {id}")]
    NotFound { resource: String, id: String },
    #[error("验证错误: {0}")]
    Validation(String),
    #[error("余额不足: 需要 {required}, 可用 {available}")]
    InsufficientBalance { required: String, available: String },
    #[error("请求过于频繁")]
    RateLimited { retry_after_secs: u64 },
    #[error("配额超限: {0}")]
    QuotaExceeded(String),
    #[error("计费错误: {0}")]
    Billing(String),
    #[error("证书错误: {0}")]
    Certificate(String),
    #[error("服务不可用: {service}")]
    ServiceUnavailable { service: String },
    #[error("熔断器开启: {0}")]
    CircuitBreakerOpen(String),
    #[error("内部错误: {0}")]
    Internal(String),
}
pub type Result<T> = std::result::Result<T, CoreError>;
impl CoreError {
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }
    pub fn not_found(resource: impl Into<String>, id: impl Into<String>) -> Self {
        Self::NotFound {
            resource: resource.into(),
            id: id.into(),
        }
    }
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }
    pub fn permission_denied(msg: impl Into<String>) -> Self {
        Self::PermissionDenied(msg.into())
    }
    pub fn insufficient_balance(required: impl Into<String>, available: impl Into<String>) -> Self {
        Self::InsufficientBalance {
            required: required.into(),
            available: available.into(),
        }
    }
    pub fn service_unavailable(service: impl Into<String>) -> Self {
        Self::ServiceUnavailable {
            service: service.into(),
        }
    }
}
impl ErrorResponse for CoreError {
    fn status_code(&self) -> StatusCode {
        match self {
            CoreError::NotFound { .. } => StatusCode::NOT_FOUND,
            CoreError::Validation(_) => StatusCode::BAD_REQUEST,
            CoreError::Auth(_) => StatusCode::UNAUTHORIZED,
            CoreError::PermissionDenied(_) => StatusCode::FORBIDDEN,
            CoreError::RateLimit(_) | CoreError::RateLimited { .. } => {
                StatusCode::TOO_MANY_REQUESTS
            }
            CoreError::InsufficientBalance { .. } => StatusCode::PAYMENT_REQUIRED,
            CoreError::QuotaExceeded(_) => StatusCode::FORBIDDEN,
            CoreError::ServiceUnavailable { .. } | CoreError::CircuitBreakerOpen(_) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            CoreError::Config(_)
            | CoreError::Database(_)
            | CoreError::Internal(_)
            | CoreError::Io(_)
            | CoreError::Network(_)
            | CoreError::Tls(_)
            | CoreError::Billing(_)
            | CoreError::Certificate(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    fn error_code(&self) -> &'static str {
        match self {
            CoreError::Database(_) => error_codes::DATABASE_ERROR,
            CoreError::Config(_) => error_codes::CONFIG_ERROR,
            CoreError::Auth(_) => error_codes::AUTH_ERROR,
            CoreError::PermissionDenied(_) => error_codes::PERMISSION_DENIED,
            CoreError::RateLimit(_) => error_codes::RATE_LIMIT_EXCEEDED,
            CoreError::NotFound { .. } => error_codes::NOT_FOUND,
            CoreError::Validation(_) => error_codes::VALIDATION_ERROR,
            CoreError::Internal(_) => error_codes::INTERNAL_ERROR,
            CoreError::Io(_) => error_codes::IO_ERROR,
            CoreError::Network(_) => error_codes::NETWORK_ERROR,
            CoreError::Tls(_) => error_codes::TLS_ERROR,
            CoreError::InsufficientBalance { .. } => error_codes::INSUFFICIENT_BALANCE,
            CoreError::RateLimited { .. } => error_codes::RATE_LIMITED,
            CoreError::QuotaExceeded(_) => error_codes::QUOTA_EXCEEDED,
            CoreError::Billing(_) => error_codes::BILLING_ERROR,
            CoreError::Certificate(_) => error_codes::CERTIFICATE_ERROR,
            CoreError::ServiceUnavailable { .. } => error_codes::SERVICE_UNAVAILABLE,
            CoreError::CircuitBreakerOpen(_) => error_codes::CIRCUIT_BREAKER_OPEN,
        }
    }
    fn details(&self) -> Option<serde_json::Value> {
        match self {
            CoreError::NotFound { resource, id } => {
                Some(json!({ "resource" : resource, "id" : id }))
            }
            CoreError::Database(_) => Some(json!({ "type" : "database_error" })),
            CoreError::InsufficientBalance {
                required,
                available,
            } => Some(json!({ "required" : required, "available" : available })),
            CoreError::RateLimited { retry_after_secs } => {
                Some(json!({ "retry_after_secs" : retry_after_secs }))
            }
            CoreError::ServiceUnavailable { service } => Some(json!({ "service" : service })),
            CoreError::CircuitBreakerOpen(_) => Some(json!({ "type" : "circuit_breaker" })),
            _ => None,
        }
    }
    fn detail(&self) -> Option<String> {
        match self {
            CoreError::Database(_) => Some("数据库操作失败".to_string()),
            CoreError::Internal(_) => Some("内部服务器错误".to_string()),
            CoreError::Io(_) => Some("IO 操作失败".to_string()),
            CoreError::Network(_) => Some("网络连接失败".to_string()),
            CoreError::Tls(_) => Some("TLS 连接失败".to_string()),
            CoreError::Billing(_) => Some("计费服务异常".to_string()),
            CoreError::Certificate(_) => Some("证书服务异常".to_string()),
            CoreError::Config(msg) => Some(msg.clone()),
            CoreError::Auth(msg) => Some(msg.clone()),
            CoreError::PermissionDenied(msg) => Some(msg.clone()),
            CoreError::RateLimit(msg) => Some(msg.clone()),
            CoreError::Validation(msg) => Some(msg.clone()),
            CoreError::NotFound { resource, id } => {
                Some(format!("资源 {resource} (id={id}) 未找到"))
            }
            CoreError::InsufficientBalance {
                required,
                available,
            } => Some(format!("余额不足，需要 {required}，可用 {available}")),
            CoreError::RateLimited { retry_after_secs } => {
                Some(format!("请求过于频繁，{retry_after_secs} 秒后重试"))
            }
            CoreError::QuotaExceeded(msg) => Some(msg.clone()),
            CoreError::ServiceUnavailable { service } => Some(format!("服务 {service} 暂不可用")),
            CoreError::CircuitBreakerOpen(msg) => Some(msg.clone()),
        }
    }
    fn safe_message(&self) -> String {
        match self {
            CoreError::Database(_) => "数据库错误".to_string(),
            CoreError::Internal(_) => "内部错误".to_string(),
            CoreError::Io(_) => "IO 错误".to_string(),
            CoreError::Network(_) => "网络错误".to_string(),
            CoreError::Tls(_) => "TLS 错误".to_string(),
            CoreError::Billing(_) => "计费错误".to_string(),
            CoreError::Certificate(_) => "证书错误".to_string(),
            _ => self.detail().unwrap_or_else(|| self.title()),
        }
    }
    fn debug_details(&self) -> Option<serde_json::Value> {
        Some(json!(
            { "error_code" : self.error_code(), "full_message" : self.to_string(), }
        ))
    }
}
impl IntoResponse for CoreError {
    fn into_response(self) -> Response {
        ErrorResponse::into_response(self)
    }
}

#[macro_export]
macro_rules! core_error_response_match {
    ($value:expr, $method:ident, { $($arms:tt)* }) => {
        match $value {
            Self::Core(error) => $crate::ErrorResponse::$method(error),
            $($arms)*
        }
    };
}

impl From<&CoreError> for mazbri_rpc::RpcError {
    fn from(err: &CoreError) -> Self {
        match err {
            CoreError::NotFound { resource, id } => mazbri_rpc::RpcError::NotFound {
                resource: resource.clone(),
                id: id.clone(),
            },
            CoreError::Validation(msg) => mazbri_rpc::RpcError::Validation {
                message: msg.clone(),
                field: None,
            },
            CoreError::Auth(msg) => mazbri_rpc::RpcError::Unauthorized {
                message: msg.clone(),
            },
            CoreError::PermissionDenied(msg) => mazbri_rpc::RpcError::Forbidden {
                message: msg.clone(),
            },
            CoreError::InsufficientBalance {
                required,
                available,
            } => mazbri_rpc::RpcError::InsufficientBalance {
                required: required.clone(),
                available: available.clone(),
            },
            CoreError::RateLimited { retry_after_secs } => mazbri_rpc::RpcError::RateLimited {
                retry_after_secs: *retry_after_secs,
            },
            CoreError::RateLimit(_) => mazbri_rpc::RpcError::RateLimited {
                retry_after_secs: 0,
            },
            CoreError::Database(_) => mazbri_rpc::RpcError::DatabaseError {
                message: "数据库错误".into(),
            },
            CoreError::ServiceUnavailable { service } => mazbri_rpc::RpcError::ServiceUnavailable {
                service: service.clone(),
                message: "服务不可用".into(),
            },
            CoreError::CircuitBreakerOpen(msg) => mazbri_rpc::RpcError::CircuitBreakerOpen {
                service: msg.clone(),
            },
            _ => mazbri_rpc::RpcError::Internal {
                message: "内部错误".into(),
            },
        }
    }
}
#[macro_export]
macro_rules! impl_error_response {
    (
        $error_type:ty { $($variant:ident $(($($field:ident),*))? $(=> ($status:expr,
        $code:expr, $title:expr))?),* $(,)? $($_wildcard:tt => ($default_status:expr,
        $default_code:expr, $default_title:expr))? }
    ) => {
        impl $crate::error::ErrorResponse for $error_type { fn status_code(& self) ->
        axum::http::StatusCode { match self { $($(<$error_type >::$variant { .. } |
        <$error_type >::$variant (..) | <$error_type >::$variant => $status,)?)* $($(_ =>
        $default_status,)?)* } } fn error_code(& self) -> &'static str { match self {
        $($(<$error_type >::$variant { .. } | <$error_type >::$variant (..) |
        <$error_type >::$variant => $code,)?)* $($(_ => $default_code,)?)* } } fn title(&
        self) -> String { match self { $($(<$error_type >::$variant { .. } | <$error_type
        >::$variant (..) | <$error_type >::$variant => $title .to_string(),)?)* $($(_ =>
        $default_title .to_string(),)?)* } } fn detail(& self) -> Option < String > {
        Some(self.to_string()) } } impl axum::response::IntoResponse for $error_type { fn
        into_response(self) -> axum::response::Response {
        $crate::error::ErrorResponse::into_response(self) } }
    };
}
