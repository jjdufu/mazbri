use crate::config_center::ConfigRepository;
use mazbri_core::error_codes;
use serde::Deserialize;
use std::fmt::Display;
use std::sync::Arc;
use tracing::{error, warn};
pub mod batch;
pub mod history;
pub mod metadata;
pub mod read;
pub mod rollback;
pub mod write;
#[derive(Clone)]
pub struct ConfigState {
    pub repo: Arc<ConfigRepository>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    #[serde(default)]
    pub service: Option<String>,
}
pub const NOT_FOUND_MESSAGE: &str = "资源未找到";
pub const VALIDATION_ERROR_MESSAGE: &str = "请求参数校验失败";
pub const INTERNAL_SERVER_ERROR_MESSAGE: &str = "内部服务器错误";
fn severity_for_error_code(error_code: &str) -> &'static str {
    match error_code {
        error_codes::NOT_FOUND | error_codes::VALIDATION_ERROR => "warn",
        _ => "error",
    }
}
pub(crate) fn log_config_error(
    event: &'static str,
    error_code: &'static str,
    request_id: &str,
    trace_id: &str,
    error: &dyn Display,
) {
    let severity = severity_for_error_code(error_code);
    match severity {
        "warn" => {
            warn!(
                service = mazbri_core::ServiceName::Cert.as_str(), event = event, severity = severity,
                error_code = error_code, request_id = request_id, trace_id = trace_id,
                error = % error, "config handler failed"
            );
        }
        _ => {
            error!(
                service = mazbri_core::ServiceName::Cert.as_str(), event = event, severity = severity,
                error_code = error_code, request_id = request_id, trace_id = trace_id,
                error = % error, "config handler failed"
            );
        }
    }
}
