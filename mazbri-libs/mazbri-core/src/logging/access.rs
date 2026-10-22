use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use std::borrow::ToOwned;
use std::collections::HashMap;
use std::process;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessRouteFields {
    pub tenant_id: Option<String>,
    pub tenant_code: Option<String>,
    pub product_id: Option<String>,
    pub supplier_id: Option<String>,
    pub supplier_code: Option<String>,
    pub channel_id: Option<String>,
    pub channel_code: Option<String>,
    pub attempt_order: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessHttpFields {
    pub method: String,
    pub protocol: String,
    pub host_header: Option<String>,
    pub path: String,
    pub route_template: String,
    pub status_code: u16,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessTimingFields {
    pub duration_ms: u64,
    pub upstream_service_time_ms: Option<u64>,
    pub dependency_wait_ms: u64,
    pub internal_ms: u64,
    pub post_response_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessNetworkFields {
    pub upstream_host: Option<String>,
    pub upstream_status_code: Option<u16>,
    pub client_ip: Option<String>,
    pub xff: Option<String>,
    pub cache_hit: bool,
    pub fallback_used: bool,
    pub retry_count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessOutcomeFields {
    pub error_code: String,
    pub error_stage: String,
    pub response_flags: String,
    /// 与 metrics `result` 对齐：ok | timeout | upstream_error | ssrf | …
    #[serde(default)]
    pub error_class: String,
}

/// 上游模式与契约（G-06）：与业务 channel 分离。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessUpstreamMetaFields {
    /// transparent | microservice | unknown
    pub upstream_mode: String,
    /// 契约 ID（见 gateway upstream_contract::known_contract_ids）
    pub contract_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessLogEvent {
    pub message: String,
    pub service: String,
    pub host: String,
    pub instance_id: String,
    pub request_id: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: String,
    pub tenant_id: String,
    pub tenant_code: String,
    pub product_id: String,
    pub supplier_id: String,
    pub supplier_code: String,
    pub channel_id: String,
    pub channel_code: String,
    pub attempt_order: u32,
    pub method: String,
    pub protocol: String,
    pub host_header: String,
    pub path: String,
    pub route_template: String,
    pub status_code: u16,
    pub duration_ms: u64,
    pub upstream_host: String,
    pub upstream_status_code: u16,
    pub upstream_service_time_ms: u64,
    pub dependency_wait_ms: u64,
    pub internal_ms: u64,
    pub post_response_ms: u64,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub client_ip: String,
    pub xff: String,
    pub user_agent: String,
    pub cache_hit: bool,
    pub fallback_used: bool,
    pub retry_count: u32,
    pub error_code: String,
    pub error_stage: String,
    pub response_flags: String,
    pub upstream_mode: String,
    pub contract_id: String,
    pub error_class: String,
}

impl AccessLogEvent {
    #[must_use]
    pub fn builder(service: impl Into<String>) -> AccessLogEventBuilder {
        AccessLogEventBuilder::new(service)
    }
}

#[derive(Debug, Clone)]
pub struct AccessLogEventBuilder {
    message: String,
    service: String,
    host: String,
    instance_id: String,
    request_id: String,
    trace_id: String,
    span_id: String,
    parent_span_id: String,
    tenant_id: Option<String>,
    tenant_code: Option<String>,
    product_id: Option<String>,
    supplier_id: Option<String>,
    supplier_code: Option<String>,
    channel_id: Option<String>,
    channel_code: Option<String>,
    attempt_order: Option<u32>,
    method: String,
    protocol: String,
    host_header: Option<String>,
    path: String,
    route_template: String,
    status_code: u16,
    duration_ms: u64,
    upstream_host: Option<String>,
    upstream_status_code: Option<u16>,
    upstream_service_time_ms: Option<u64>,
    dependency_wait_ms: u64,
    internal_ms: u64,
    post_response_ms: u64,
    request_bytes: u64,
    response_bytes: u64,
    client_ip: Option<String>,
    xff: Option<String>,
    user_agent: Option<String>,
    cache_hit: bool,
    fallback_used: bool,
    retry_count: u32,
    error_code: String,
    error_stage: String,
    response_flags: String,
    upstream_mode: String,
    contract_id: String,
    error_class: String,
}

impl AccessLogEventBuilder {
    #[must_use]
    pub fn new(service: impl Into<String>) -> Self {
        let service = service.into();
        let context = TraceContext::generated();
        Self {
            message: "request_completed".to_string(),
            service,
            host: resolve_host(),
            instance_id: resolve_instance_id(),
            request_id: context.request_id.to_string(),
            trace_id: context.trace_id.to_string(),
            span_id: context.span_id.to_string(),
            parent_span_id: context.parent_span_id.to_string(),
            tenant_id: None,
            tenant_code: None,
            product_id: None,
            supplier_id: None,
            supplier_code: None,
            channel_id: None,
            channel_code: None,
            attempt_order: None,
            method: String::new(),
            protocol: String::new(),
            host_header: None,
            path: String::new(),
            route_template: String::new(),
            status_code: 0,
            duration_ms: 0,
            upstream_host: None,
            upstream_status_code: None,
            upstream_service_time_ms: None,
            dependency_wait_ms: 0,
            internal_ms: 0,
            post_response_ms: 0,
            request_bytes: 0,
            response_bytes: 0,
            client_ip: None,
            xff: None,
            user_agent: None,
            cache_hit: false,
            fallback_used: false,
            retry_count: 0,
            error_code: String::new(),
            error_stage: String::new(),
            response_flags: "-".to_string(),
            upstream_mode: String::new(),
            contract_id: String::new(),
            error_class: String::new(),
        }
    }

    #[must_use]
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    #[must_use]
    pub fn trace_context(mut self, context: &TraceContext) -> Self {
        self.request_id = context.request_id.to_string();
        self.trace_id = context.trace_id.to_string();
        self.span_id = context.span_id.to_string();
        self.parent_span_id = context.parent_span_id.to_string();
        self
    }

    #[must_use]
    pub fn route(mut self, fields: AccessRouteFields) -> Self {
        self.tenant_id = fields.tenant_id;
        self.tenant_code = fields.tenant_code;
        self.product_id = fields.product_id;
        self.supplier_id = fields.supplier_id;
        self.supplier_code = fields.supplier_code;
        self.channel_id = fields.channel_id;
        self.channel_code = fields.channel_code;
        self.attempt_order = fields.attempt_order;
        self
    }

    #[must_use]
    pub fn http(mut self, fields: AccessHttpFields) -> Self {
        self.method = fields.method;
        self.protocol = fields.protocol;
        self.host_header = fields.host_header;
        self.path = fields.path;
        self.route_template = fields.route_template;
        self.status_code = fields.status_code;
        self.request_bytes = fields.request_bytes;
        self.response_bytes = fields.response_bytes;
        self.user_agent = fields.user_agent;
        self
    }

    #[must_use]
    pub fn timing(mut self, fields: AccessTimingFields) -> Self {
        self.duration_ms = fields.duration_ms;
        self.upstream_service_time_ms = fields.upstream_service_time_ms;
        self.dependency_wait_ms = fields.dependency_wait_ms;
        self.internal_ms = fields.internal_ms;
        self.post_response_ms = fields.post_response_ms;
        self
    }

    #[must_use]
    pub fn network(mut self, fields: AccessNetworkFields) -> Self {
        self.upstream_host = fields.upstream_host;
        self.upstream_status_code = fields.upstream_status_code;
        self.client_ip = fields.client_ip;
        self.xff = fields.xff;
        self.cache_hit = fields.cache_hit;
        self.fallback_used = fields.fallback_used;
        self.retry_count = fields.retry_count;
        self
    }

    #[must_use]
    pub fn outcome(mut self, fields: AccessOutcomeFields) -> Self {
        self.error_code = fields.error_code;
        self.error_stage = fields.error_stage;
        self.response_flags = fields.response_flags;
        if !fields.error_class.is_empty() {
            self.error_class = fields.error_class;
        }
        self
    }

    #[must_use]
    pub fn upstream_meta(mut self, fields: AccessUpstreamMetaFields) -> Self {
        self.upstream_mode = fields.upstream_mode;
        self.contract_id = fields.contract_id;
        self
    }

    #[must_use]
    pub fn build(self) -> AccessLogEvent {
        AccessLogEvent {
            message: self.message,
            service: self.service,
            host: self.host,
            instance_id: self.instance_id,
            request_id: self.request_id,
            trace_id: self.trace_id,
            span_id: self.span_id,
            parent_span_id: self.parent_span_id,
            tenant_id: self.tenant_id.unwrap_or_default(),
            tenant_code: self.tenant_code.unwrap_or_default(),
            product_id: self.product_id.unwrap_or_default(),
            supplier_id: self.supplier_id.unwrap_or_default(),
            supplier_code: self.supplier_code.unwrap_or_default(),
            channel_id: self.channel_id.unwrap_or_default(),
            channel_code: self.channel_code.unwrap_or_default(),
            attempt_order: self.attempt_order.unwrap_or_default(),
            method: self.method,
            protocol: self.protocol,
            host_header: self.host_header.unwrap_or_default(),
            path: self.path,
            route_template: self.route_template,
            status_code: self.status_code,
            duration_ms: self.duration_ms,
            upstream_host: self.upstream_host.unwrap_or_default(),
            upstream_status_code: self.upstream_status_code.unwrap_or_default(),
            upstream_service_time_ms: self.upstream_service_time_ms.unwrap_or_default(),
            dependency_wait_ms: self.dependency_wait_ms,
            internal_ms: self.internal_ms,
            post_response_ms: self.post_response_ms,
            request_bytes: self.request_bytes,
            response_bytes: self.response_bytes,
            client_ip: self.client_ip.unwrap_or_default(),
            xff: self.xff.unwrap_or_default(),
            user_agent: self.user_agent.unwrap_or_default(),
            cache_hit: self.cache_hit,
            fallback_used: self.fallback_used,
            retry_count: self.retry_count,
            error_code: self.error_code,
            error_stage: self.error_stage,
            response_flags: self.response_flags,
            upstream_mode: self.upstream_mode,
            contract_id: self.contract_id,
            error_class: self.error_class,
        }
    }
}

/// Cross-service correlation IDs. Fields are `Arc<str>` so hot-path clones are
/// reference-count bumps rather than full string allocations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    pub request_id: Arc<str>,
    pub trace_id: Arc<str>,
    pub span_id: Arc<str>,
    pub parent_span_id: Arc<str>,
}

impl TraceContext {
    #[must_use]
    pub fn generated() -> Self {
        // request_id: standard hyphenated UUID (platform serial / 流水号).
        // trace_id: 32-hex simple form for W3C `traceparent` compatibility.
        let request_id: Arc<str> = Uuid::new_v4().to_string().into();
        let trace_id: Arc<str> = Uuid::new_v4().simple().to_string().into();
        let span_id: Arc<str> = current_span_id().into();
        Self {
            request_id,
            trace_id,
            span_id,
            parent_span_id: Arc::from(""),
        }
    }

    #[must_use]
    pub fn traceparent_value(&self) -> String {
        format!("00-{}-{}-01", self.trace_id, self.span_id)
    }

    /// Insert W3C/`x-*` correlation headers into an existing map (avoids an extra map alloc).
    pub fn write_propagate_headers(&self, headers: &mut HashMap<String, String>) {
        headers.insert("x-request-id".to_string(), self.request_id.to_string());
        headers.insert("x-trace-id".to_string(), self.trace_id.to_string());
        headers.insert("x-span-id".to_string(), self.span_id.to_string());
        headers.insert(
            "x-parent-span-id".to_string(),
            self.parent_span_id.to_string(),
        );
        headers.insert("traceparent".to_string(), self.traceparent_value());
    }

    #[must_use]
    pub fn propagate_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::with_capacity(5);
        self.write_propagate_headers(&mut headers);
        headers
    }
}

pub trait TraceHeaderSource {
    fn header_value(&self, name: &str) -> Option<String>;
}

impl TraceHeaderSource for HeaderMap {
    fn header_value(&self, name: &str) -> Option<String> {
        self.get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }
}

impl TraceHeaderSource for HashMap<String, String> {
    fn header_value(&self, name: &str) -> Option<String> {
        self.get(name)
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }
}

/// Normalize a correlation id into a **hyphenated UUID** string when possible.
///
/// - Accepts standard UUID and 32-hex "simple" form
/// - Always returns lowercase `8-4-4-4-12`
/// - Empty / unparsable non-UUID values: generate a new UUID (empty) or keep raw
///   opaque ids that are not UUID-shaped (rare custom clients)
#[must_use]
pub fn normalize_request_id_uuid(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Uuid::new_v4().to_string();
    }
    match Uuid::parse_str(trimmed) {
        Ok(id) => id.to_string(),
        Err(_) => trimmed.to_string(),
    }
}

#[must_use]
pub fn parse_trace_context<H: TraceHeaderSource>(headers: &H) -> TraceContext {
    let raw_request_id = headers
        .header_value("x-request-id")
        .or_else(|| headers.header_value("request-id"))
        .or_else(|| headers.header_value("request_id"));
    let request_id: Arc<str> = match raw_request_id {
        Some(value) => normalize_request_id_uuid(&value).into(),
        None => Uuid::new_v4().to_string().into(),
    };

    let traceparent = headers.header_value("traceparent");
    let traceparent_parts = traceparent
        .as_deref()
        .and_then(parse_traceparent_value)
        .unwrap_or_else(|| ("".to_string(), "".to_string()));
    let tp_trace_id = traceparent_parts.0;
    let parent_from_traceparent = traceparent_parts.1;

    let trace_id: Arc<str> = if !tp_trace_id.is_empty() {
        // W3C trace-id is 32-hex; keep as provided from traceparent.
        tp_trace_id.into()
    } else if let Some(value) = headers.header_value("x-trace-id") {
        // Prefer simple form when the value is a UUID so traceparent stays valid.
        match Uuid::parse_str(value.trim()) {
            Ok(id) => id.simple().to_string().into(),
            Err(_) => value.into(),
        }
    } else {
        // No client trace: derive W3C-compatible 32-hex from request_id when possible.
        match Uuid::parse_str(request_id.as_ref()) {
            Ok(id) => id.simple().to_string().into(),
            Err(_) => Arc::clone(&request_id),
        }
    };

    let parent_span_id: Arc<str> = headers
        .header_value("x-span-id")
        .or(if !parent_from_traceparent.is_empty() {
            Some(parent_from_traceparent)
        } else {
            None
        })
        .or_else(|| headers.header_value("x-parent-span-id"))
        .map(Arc::<str>::from)
        .unwrap_or_else(|| Arc::from(""));

    TraceContext {
        request_id,
        trace_id,
        span_id: Arc::from(current_span_id()),
        parent_span_id,
    }
}

#[must_use]
pub fn current_span_id() -> String {
    let bytes = Uuid::new_v4().into_bytes();
    hex::encode(&bytes[..8])
}

#[must_use]
pub fn normalize_upstream_host(url_or_addr: &str) -> Option<String> {
    let trimmed = url_or_addr.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

#[must_use]
pub fn response_flags_for_error(error_code: &str, stage: &str, status_code: u16) -> String {
    if error_code.trim().is_empty() || (200..400).contains(&status_code) {
        return "-".to_string();
    }
    let code = error_code.trim().to_ascii_uppercase();
    let stage = stage.trim().to_ascii_lowercase();
    if code.contains("TIMEOUT") || stage.contains("timeout") {
        return "UT".to_string();
    }
    if code.contains("RATE_LIMIT") {
        return "RL".to_string();
    }
    if code.contains("CIRCUIT") {
        return "CB".to_string();
    }
    if code.contains("BAD_GATEWAY") || code.contains("UPSTREAM") || stage.contains("upstream") {
        return "UF".to_string();
    }
    if code.contains("INVALID") || stage.contains("validation") {
        return "IP".to_string();
    }
    "UF".to_string()
}

pub fn emit_access_log(target: &str, event: AccessLogEvent) {
    match target {
        "gateway.access" => emit_access_log_gateway(event),
        "billing.access" => emit_access_log_billing(event),
        // transparent / 免代码出站：与 bootstrap log_routes include_targets 对齐
        "mazbri-bypass.access" | "bypass.access" => emit_access_log_bypass(event),
        _ => emit_access_log_generic(target, event),
    }
}

macro_rules! emit_access_log_for_target {
    ($target:literal, $event:ident) => {
        info!(
            target: $target,
            message = %$event.message,
            service = %$event.service,
            host = %$event.host,
            instance_id = %$event.instance_id,
            request_id = %$event.request_id,
            trace_id = %$event.trace_id,
            span_id = %$event.span_id,
            parent_span_id = %$event.parent_span_id,
            tenant_id = %$event.tenant_id,
            tenant_code = %$event.tenant_code,
            product_id = %$event.product_id,
            supplier_id = %$event.supplier_id,
            supplier_code = %$event.supplier_code,
            channel_id = %$event.channel_id,
            channel_code = %$event.channel_code,
            attempt_order = $event.attempt_order,
            method = %$event.method,
            protocol = %$event.protocol,
            host_header = %$event.host_header,
            path = %$event.path,
            route_template = %$event.route_template,
            status_code = $event.status_code,
            duration_ms = $event.duration_ms,
            upstream_host = %$event.upstream_host,
            upstream_status_code = $event.upstream_status_code,
            upstream_service_time_ms = $event.upstream_service_time_ms,
            dependency_wait_ms = $event.dependency_wait_ms,
            internal_ms = $event.internal_ms,
            post_response_ms = $event.post_response_ms,
            request_bytes = $event.request_bytes,
            response_bytes = $event.response_bytes,
            client_ip = %$event.client_ip,
            xff = %$event.xff,
            user_agent = %$event.user_agent,
            cache_hit = $event.cache_hit,
            fallback_used = $event.fallback_used,
            retry_count = $event.retry_count,
            error_code = %$event.error_code,
            error_stage = %$event.error_stage,
            response_flags = %$event.response_flags,
            upstream_mode = %$event.upstream_mode,
            contract_id = %$event.contract_id,
            error_class = %$event.error_class,
        );
    };
}

fn emit_access_log_gateway(event: AccessLogEvent) {
    emit_access_log_for_target!("gateway.access", event);
}

fn emit_access_log_billing(event: AccessLogEvent) {
    emit_access_log_for_target!("billing.access", event);
}

fn emit_access_log_bypass(event: AccessLogEvent) {
    emit_access_log_for_target!("mazbri-bypass.access", event);
}

fn emit_access_log_generic(target: &str, event: AccessLogEvent) {
    info!(
        target: "access",
        log_target = %target,
        message = %event.message,
        service = %event.service,
        host = %event.host,
        instance_id = %event.instance_id,
        request_id = %event.request_id,
        trace_id = %event.trace_id,
        span_id = %event.span_id,
        parent_span_id = %event.parent_span_id,
        tenant_id = %event.tenant_id,
        tenant_code = %event.tenant_code,
        product_id = %event.product_id,
        supplier_id = %event.supplier_id,
        supplier_code = %event.supplier_code,
        channel_id = %event.channel_id,
        channel_code = %event.channel_code,
        attempt_order = event.attempt_order,
        method = %event.method,
        protocol = %event.protocol,
        host_header = %event.host_header,
        path = %event.path,
        route_template = %event.route_template,
        status_code = event.status_code,
        duration_ms = event.duration_ms,
        upstream_host = %event.upstream_host,
        upstream_status_code = event.upstream_status_code,
        upstream_service_time_ms = event.upstream_service_time_ms,
        dependency_wait_ms = event.dependency_wait_ms,
        internal_ms = event.internal_ms,
        post_response_ms = event.post_response_ms,
        request_bytes = event.request_bytes,
        response_bytes = event.response_bytes,
        client_ip = %event.client_ip,
        xff = %event.xff,
        user_agent = %event.user_agent,
        cache_hit = event.cache_hit,
        fallback_used = event.fallback_used,
        retry_count = event.retry_count,
        error_code = %event.error_code,
        error_stage = %event.error_stage,
        response_flags = %event.response_flags,
        upstream_mode = %event.upstream_mode,
        contract_id = %event.contract_id,
        error_class = %event.error_class,
    );
}

fn resolve_host() -> String {
    hostname::get()
        .ok()
        .and_then(|value| value.into_string().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown-host".to_string())
}

fn resolve_instance_id() -> String {
    for key in ["MAZBRI_LOG_INSTANCE_ID", "POD_NAME", "HOSTNAME"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim();
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }
    process::id().to_string()
}

fn parse_traceparent_value(value: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() < 4 {
        return None;
    }
    let trace_id = parts.get(1)?.trim();
    let parent_span_id = parts.get(2)?.trim();
    if trace_id.is_empty() {
        return None;
    }
    Some((trace_id.to_string(), parent_span_id.to_string()))
}

#[cfg(test)]
mod request_id_uuid_tests {
    use super::{TraceContext, normalize_request_id_uuid, parse_trace_context};
    use std::collections::HashMap;
    use uuid::Uuid;

    #[test]
    fn normalize_accepts_simple_and_hyphenated() {
        let simple = "6e7f448edae94db78e5e9f87f1402eaa";
        let hyphen = "6e7f448e-dae9-4db7-8e5e-9f87f1402eaa";
        assert_eq!(normalize_request_id_uuid(simple), hyphen);
        assert_eq!(normalize_request_id_uuid(hyphen), hyphen);
        assert_eq!(
            normalize_request_id_uuid("6E7F448EDAE94DB78E5E9F87F1402EAA"),
            hyphen
        );
    }

    #[test]
    fn generated_request_id_is_hyphenated_uuid() {
        let ctx = TraceContext::generated();
        assert!(Uuid::parse_str(&ctx.request_id).is_ok());
        assert!(ctx.request_id.contains('-'));
        assert_eq!(ctx.request_id.len(), 36);
        // W3C-friendly 32-hex trace id
        assert_eq!(ctx.trace_id.len(), 32);
        assert!(!ctx.trace_id.contains('-'));
    }

    #[test]
    fn parse_normalizes_client_simple_request_id() {
        let mut headers = HashMap::new();
        headers.insert(
            "x-request-id".to_string(),
            "6e7f448edae94db78e5e9f87f1402eaa".to_string(),
        );
        let ctx = parse_trace_context(&headers);
        assert_eq!(
            ctx.request_id.as_ref(),
            "6e7f448e-dae9-4db7-8e5e-9f87f1402eaa"
        );
        assert_eq!(ctx.trace_id.as_ref(), "6e7f448edae94db78e5e9f87f1402eaa");
    }
}
