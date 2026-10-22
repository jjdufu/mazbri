use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RpcError {
    NotFound {
        resource: String,
        id: String,
    },
    Validation {
        message: String,
        field: Option<String>,
    },
    Unauthorized {
        message: String,
    },
    Forbidden {
        message: String,
    },
    InsufficientBalance {
        required: String,
        available: String,
    },
    RateLimited {
        retry_after_secs: u64,
    },
    DatabaseError {
        message: String,
    },
    ServiceUnavailable {
        service: String,
        message: String,
    },
    CircuitBreakerOpen {
        service: String,
    },
    Internal {
        message: String,
    },
}
impl RpcError {
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            RpcError::NotFound { resource, id } => {
                format!("资源 {resource} (id={id}) 未找到")
            }
            RpcError::Validation { message, .. }
            | RpcError::Unauthorized { message }
            | RpcError::Forbidden { message }
            | RpcError::DatabaseError { message }
            | RpcError::ServiceUnavailable { message, .. }
            | RpcError::Internal { message } => message.clone(),
            RpcError::InsufficientBalance {
                required,
                available,
            } => {
                format!("余额不足，需要 {required}，可用 {available}")
            }
            RpcError::RateLimited { retry_after_secs } => {
                format!("请求过于频繁，{retry_after_secs} 秒后重试")
            }
            RpcError::CircuitBreakerOpen { service } => {
                format!("熔断器开启: {service}")
            }
        }
    }
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            RpcError::NotFound { .. } => "NOT_FOUND",
            RpcError::Validation { .. } => "VALIDATION_ERROR",
            RpcError::Unauthorized { .. } => "AUTH_ERROR",
            RpcError::Forbidden { .. } => "PERMISSION_DENIED",
            RpcError::InsufficientBalance { .. } => "INSUFFICIENT_BALANCE",
            RpcError::RateLimited { .. } => "RATE_LIMITED",
            RpcError::DatabaseError { .. } => "DATABASE_ERROR",
            RpcError::ServiceUnavailable { .. } => "SERVICE_UNAVAILABLE",
            RpcError::CircuitBreakerOpen { .. } => "CIRCUIT_BREAKER_OPEN",
            RpcError::Internal { .. } => "INTERNAL_ERROR",
        }
    }
}
