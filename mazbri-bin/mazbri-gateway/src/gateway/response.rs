use axum::http::StatusCode;
use axum::{
    Json,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}
#[derive(Debug)]
pub enum ProxyError {
    Forbidden(String),
    NotFound(String),
    BadRequest(String),
    Conflict(String),
    PayloadTooLarge(String),
    /// Channel or platform rate limit; carries suggested Retry-After seconds.
    TooManyRequests {
        message: String,
        retry_after_secs: u64,
    },
    ServiceUnavailable(String),
    GatewayTimeout(String),
    BadGateway(String),
    InternalError(String),
}
impl std::fmt::Display for ProxyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProxyError::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            ProxyError::NotFound(msg) => write!(f, "Not Found: {}", msg),
            ProxyError::BadRequest(msg) => write!(f, "Bad Request: {}", msg),
            ProxyError::Conflict(msg) => write!(f, "Conflict: {}", msg),
            ProxyError::PayloadTooLarge(msg) => write!(f, "Payload Too Large: {}", msg),
            ProxyError::TooManyRequests { message, .. } => {
                write!(f, "Too Many Requests: {}", message)
            }
            ProxyError::ServiceUnavailable(msg) => write!(f, "Service Unavailable: {}", msg),
            ProxyError::GatewayTimeout(msg) => write!(f, "Gateway Timeout: {}", msg),
            ProxyError::BadGateway(msg) => write!(f, "Bad Gateway: {}", msg),
            ProxyError::InternalError(msg) => write!(f, "Internal Error: {}", msg),
        }
    }
}
impl ProxyError {
    pub fn error_code(&self) -> &'static str {
        match self {
            ProxyError::Forbidden(_) => "E4030",
            ProxyError::NotFound(_) => "E4040",
            ProxyError::BadRequest(_) => "E4000",
            ProxyError::Conflict(_) => "E4090",
            ProxyError::PayloadTooLarge(_) => "E4130",
            ProxyError::TooManyRequests { .. } => "E4290",
            ProxyError::ServiceUnavailable(_) => "E5030",
            ProxyError::GatewayTimeout(_) => "E5040",
            ProxyError::BadGateway(_) => "E5020",
            ProxyError::InternalError(_) => "E5000",
        }
    }
    pub fn should_sanitize(&self) -> bool {
        matches!(
            self,
            ProxyError::InternalError(_)
                | ProxyError::BadGateway(_)
                | ProxyError::GatewayTimeout(_)
        )
    }
    pub fn sanitized_message(&self) -> String {
        match self {
            ProxyError::InternalError(_) => "Internal Server Error".to_string(),
            ProxyError::BadGateway(_) => "Upstream service unavailable".to_string(),
            // 对外稳定文案；内部细节仅写日志
            ProxyError::GatewayTimeout(_) => {
                "Gateway Timeout: upstream or channel wait exceeded limit".to_string()
            }
            _ => self.to_string(),
        }
    }
}
/// Build a unified gateway JSON error envelope (success/status/error/request_id).
pub fn gateway_error_response(
    status: StatusCode,
    error_code: &str,
    message: impl Into<String>,
    request_id: Option<String>,
) -> Response {
    gateway_error_response_with_retry(status, error_code, message, request_id, None)
}

pub fn gateway_error_response_with_retry(
    status: StatusCode,
    error_code: &str,
    message: impl Into<String>,
    request_id: Option<String>,
    retry_after_secs: Option<u64>,
) -> Response {
    let request_id = request_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let resp = ProxyResponse::<()> {
        success: false,
        data: None,
        error: Some(format!("[{}] {}", error_code, message.into())),
        status: status.as_u16(),
        request_id: Some(request_id),
    };
    let mut response = (status, Json(resp)).into_response();
    let retry_value = match (status, retry_after_secs) {
        (StatusCode::TOO_MANY_REQUESTS, Some(secs)) => Some(secs.max(1).to_string()),
        (StatusCode::SERVICE_UNAVAILABLE, _) => Some("1".to_string()),
        _ => None,
    };
    if let Some(value) = retry_value {
        if let Ok(header_value) = axum::http::HeaderValue::from_str(&value) {
            response
                .headers_mut()
                .insert(axum::http::header::RETRY_AFTER, header_value);
        }
    }
    response
}
impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let (status, retry_after_secs) = match &self {
            ProxyError::Forbidden(_) => (StatusCode::FORBIDDEN, None),
            ProxyError::NotFound(_) => (StatusCode::NOT_FOUND, None),
            ProxyError::BadRequest(_) => (StatusCode::BAD_REQUEST, None),
            ProxyError::Conflict(_) => (StatusCode::CONFLICT, None),
            ProxyError::PayloadTooLarge(_) => (StatusCode::PAYLOAD_TOO_LARGE, None),
            ProxyError::TooManyRequests {
                retry_after_secs, ..
            } => (StatusCode::TOO_MANY_REQUESTS, Some(*retry_after_secs)),
            ProxyError::ServiceUnavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, None),
            ProxyError::GatewayTimeout(_) => (StatusCode::GATEWAY_TIMEOUT, None),
            ProxyError::BadGateway(_) => (StatusCode::BAD_GATEWAY, None),
            ProxyError::InternalError(_) => (StatusCode::INTERNAL_SERVER_ERROR, None),
        };
        let request_id = Uuid::new_v4().to_string();
        let error_code = self.error_code();
        if self.should_sanitize() {
            tracing::error!(
                request_id = %request_id,
                error_code = error_code,
                status_code = status.as_u16(),
                error = %self.to_string(),
                "Gateway error occurred"
            );
        }
        let message = if self.should_sanitize() {
            self.sanitized_message()
        } else {
            match &self {
                ProxyError::TooManyRequests { message, .. } => message.clone(),
                _ => self.to_string(),
            }
        };
        gateway_error_response_with_retry(
            status,
            error_code,
            message,
            Some(request_id),
            retry_after_secs,
        )
    }
}
