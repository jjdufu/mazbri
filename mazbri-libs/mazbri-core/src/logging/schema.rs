use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardLogEvent {
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "level")]
    pub level: String,
    #[serde(rename = "service")]
    pub service: String,
    #[serde(rename = "message")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_order: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_total_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_total_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_wait_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_response_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
}
impl StandardLogEvent {
    #[must_use]
    pub fn new(
        level: impl Into<String>,
        service: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            level: level.into(),
            service: service.into(),
            message: message.into(),
            request_id: None,
            tenant_id: None,
            product_id: None,
            supplier_id: None,
            supplier_code: None,
            supplier_name: None,
            channel_id: None,
            channel_code: None,
            channel_name: None,
            upstream_url: None,
            attempt_order: None,
            response_result: None,
            trace_id: None,
            span_id: None,
            target: None,
            filename: None,
            line_number: None,
            method: None,
            path: None,
            status_code: None,
            customer_total_ms: None,
            service_total_ms: None,
            dependency_wait_ms: None,
            internal_ms: None,
            post_response_ms: None,
            source_ip: None,
            user_agent: None,
        }
    }
    #[must_use]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }
    #[must_use]
    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }
    #[must_use]
    pub fn with_product_id(mut self, product_id: impl Into<String>) -> Self {
        self.product_id = Some(product_id.into());
        self
    }
    #[must_use]
    pub fn with_supplier_id(mut self, supplier_id: impl Into<String>) -> Self {
        self.supplier_id = Some(supplier_id.into());
        self
    }
    #[must_use]
    pub fn with_supplier_code(mut self, supplier_code: impl Into<String>) -> Self {
        self.supplier_code = Some(supplier_code.into());
        self
    }
    #[must_use]
    pub fn with_supplier_name(mut self, supplier_name: impl Into<String>) -> Self {
        self.supplier_name = Some(supplier_name.into());
        self
    }
    #[must_use]
    pub fn with_channel_id(mut self, channel_id: impl Into<String>) -> Self {
        self.channel_id = Some(channel_id.into());
        self
    }
    #[must_use]
    pub fn with_channel_code(mut self, channel_code: impl Into<String>) -> Self {
        self.channel_code = Some(channel_code.into());
        self
    }
    #[must_use]
    pub fn with_channel_name(mut self, channel_name: impl Into<String>) -> Self {
        self.channel_name = Some(channel_name.into());
        self
    }
    #[must_use]
    pub fn with_upstream_url(mut self, upstream_url: impl Into<String>) -> Self {
        self.upstream_url = Some(upstream_url.into());
        self
    }
    #[must_use]
    pub fn with_attempt_order(mut self, attempt_order: u32) -> Self {
        self.attempt_order = Some(attempt_order);
        self
    }
    #[must_use]
    pub fn with_response_result(mut self, response_result: impl Into<String>) -> Self {
        self.response_result = Some(response_result.into());
        self
    }
    #[must_use]
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }
    #[must_use]
    pub fn with_span_id(mut self, span_id: impl Into<String>) -> Self {
        self.span_id = Some(span_id.into());
        self
    }
    #[must_use]
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }
    #[must_use]
    pub fn with_filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }
    #[must_use]
    pub fn with_line_number(mut self, line_number: u32) -> Self {
        self.line_number = Some(line_number);
        self
    }
    #[must_use]
    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = Some(method.into());
        self
    }
    #[must_use]
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
    #[must_use]
    pub fn with_status_code(mut self, status_code: u16) -> Self {
        self.status_code = Some(status_code);
        self
    }
    #[must_use]
    pub fn with_customer_total_ms(mut self, customer_total_ms: u64) -> Self {
        self.customer_total_ms = Some(customer_total_ms);
        self
    }
    #[must_use]
    pub fn with_service_total_ms(mut self, service_total_ms: u64) -> Self {
        self.service_total_ms = Some(service_total_ms);
        self
    }
    #[must_use]
    pub fn with_dependency_wait_ms(mut self, dependency_wait_ms: u64) -> Self {
        self.dependency_wait_ms = Some(dependency_wait_ms);
        self
    }
    #[must_use]
    pub fn with_internal_ms(mut self, internal_ms: u64) -> Self {
        self.internal_ms = Some(internal_ms);
        self
    }
    #[must_use]
    pub fn with_post_response_ms(mut self, post_response_ms: u64) -> Self {
        self.post_response_ms = Some(post_response_ms);
        self
    }
    #[must_use]
    pub fn with_source_ip(mut self, source_ip: impl Into<String>) -> Self {
        self.source_ip = Some(source_ip.into());
        self
    }
    #[must_use]
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
pub mod log_level {
    pub const TRACE: &str = "TRACE";
    pub const DEBUG: &str = "DEBUG";
    pub const INFO: &str = "INFO";
    pub const WARN: &str = "WARN";
    pub const ERROR: &str = "ERROR";
}

#[macro_export]
macro_rules! standard_log_event_info {
    (target: $target:expr, $event:expr) => {{
        let event = &$event;
        tracing::info!(
            target: $target,
            timestamp = %event.timestamp.to_rfc3339(),
            level = %event.level,
            service = %event.service,
            message = %event.message,
            request_id = event.request_id.as_deref().unwrap_or(""),
            tenant_id = event.tenant_id.as_deref().unwrap_or(""),
            product_id = event.product_id.as_deref().unwrap_or(""),
            supplier_id = event.supplier_id.as_deref().unwrap_or(""),
            supplier_code = event.supplier_code.as_deref().unwrap_or(""),
            supplier_name = event.supplier_name.as_deref().unwrap_or(""),
            channel_id = event.channel_id.as_deref().unwrap_or(""),
            channel_code = event.channel_code.as_deref().unwrap_or(""),
            channel_name = event.channel_name.as_deref().unwrap_or(""),
            upstream_url = event.upstream_url.as_deref().unwrap_or(""),
            attempt_order = event.attempt_order.unwrap_or_default(),
            response_result = event.response_result.as_deref().unwrap_or(""),
            trace_id = event.trace_id.as_deref().unwrap_or(""),
            span_id = event.span_id.as_deref().unwrap_or(""),
            event_target = event.target.as_deref().unwrap_or(""),
            event_filename = event.filename.as_deref().unwrap_or(""),
            event_line_number = event.line_number.unwrap_or_default(),
            method = event.method.as_deref().unwrap_or(""),
            path = event.path.as_deref().unwrap_or(""),
            status_code = event.status_code.unwrap_or_default(),
            customer_total_ms = event.customer_total_ms.unwrap_or_default(),
            service_total_ms = event.service_total_ms.unwrap_or_default(),
            dependency_wait_ms = event.dependency_wait_ms.unwrap_or_default(),
            internal_ms = event.internal_ms.unwrap_or_default(),
            post_response_ms = event.post_response_ms.unwrap_or_default(),
            source_ip = event.source_ip.as_deref().unwrap_or(""),
            user_agent = event.user_agent.as_deref().unwrap_or(""),
        );
    }};
}
