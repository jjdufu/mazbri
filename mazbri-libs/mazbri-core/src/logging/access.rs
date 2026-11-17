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
    /// 调用方 API Key ID（网关取自 JWT claim `api_key_id`）；无调用方上下文时为 `None`。
    pub api_key_id: Option<String>,
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
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
    /// 本次请求的每一次重发各记一个原因（`channel_switch` / `config_cache_miss` / `transport` /
    /// `auth_rejected:*` …）；访问日志 `retry_count` 即其长度。
    pub retry_reasons: Vec<String>,
    /// 上游响应回来的流水号（bypass 按渠道 egress spec `response.request_id_header` 读取）。
    pub upstream_request_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessOutcomeFields {
    pub error_code: String,
    pub error_stage: String,
    pub response_flags: String,
    /// 错误来源：`client` | `gateway` | `upstream`；成功为空。见 [`classify_outcome`]。
    pub error_source: String,
    /// 与 metrics `result` 对齐：ok | timeout | `upstream_error` | ssrf | …
    #[serde(default)]
    pub error_class: String,
}

/// 上游模式与契约（G-06）：与业务 channel 分离。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessUpstreamMetaFields {
    /// transparent | microservice | unknown
    pub upstream_mode: String,
    /// 契约 ID（见 gateway `upstream_contract::known_contract_ids`）
    pub contract_id: String,
}

/// Billing correlation (L3 reconciliation): the usage event this request produced.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessBillingFields {
    /// Usage event id (same key as `usage_outbox.event_id` / `usage_records.event_id`).
    pub event_id: String,
    /// Whether this request is billable (a usage charge is expected).
    pub billable: bool,
    /// Charged amount (decimal string) when billable.
    pub price: String,
    /// `BillingCode` numeric value.
    pub billing_code: i16,
    /// Upstream cost price (decimal string) of the live upstream call; `0` when no live call.
    pub cost: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessLogEvent {
    pub message: String,
    pub service: String,
    pub host: String,
    pub instance_id: String,
    pub request_id: String,
    /// 客户传入但不合规（非 `[A-Za-z0-9._-]{1,128}`）而被替换的原始 `X-Request-Id`（截断）；合规或未传时为空。
    pub client_request_id: String,
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
    pub api_key_id: String,
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
    /// 逗号分隔，长度（项数）等于 `retry_count`。
    pub retry_reasons: String,
    pub upstream_request_id: String,
    pub error_code: String,
    pub error_stage: String,
    pub response_flags: String,
    pub error_source: String,
    pub upstream_mode: String,
    pub contract_id: String,
    pub error_class: String,
    #[serde(default)]
    pub event_id: String,
    #[serde(default)]
    pub billable: bool,
    #[serde(default)]
    pub price: String,
    #[serde(default)]
    pub billing_code: i16,
    #[serde(default)]
    pub cost: String,
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
    client_request_id: String,
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
    api_key_id: Option<String>,
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
    retry_reasons: Vec<String>,
    upstream_request_id: Option<String>,
    error_code: String,
    error_stage: String,
    response_flags: String,
    error_source: String,
    upstream_mode: String,
    contract_id: String,
    error_class: String,
    billing: AccessBillingFields,
}

impl AccessLogEventBuilder {
    #[must_use]
    pub fn new(service: impl Into<String>) -> Self {
        let service = service.into();
        let context = current_request_trace().unwrap_or_else(TraceContext::current);
        Self {
            message: "request_completed".to_string(),
            service,
            host: resolve_host(),
            instance_id: resolve_instance_id(),
            request_id: context.request_id.to_string(),
            client_request_id: context.client_request_id.to_string(),
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
            api_key_id: None,
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
            retry_reasons: Vec::new(),
            upstream_request_id: None,
            error_code: String::new(),
            error_stage: String::new(),
            response_flags: "-".to_string(),
            error_source: String::new(),
            upstream_mode: String::new(),
            contract_id: String::new(),
            error_class: String::new(),
            billing: AccessBillingFields::default(),
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
        self.client_request_id = context.client_request_id.to_string();
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
        self.api_key_id = fields.api_key_id;
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
        self.retry_reasons = fields.retry_reasons;
        self.upstream_request_id = fields.upstream_request_id;
        self
    }

    #[must_use]
    pub fn outcome(mut self, fields: AccessOutcomeFields) -> Self {
        self.error_code = fields.error_code;
        self.error_stage = fields.error_stage;
        self.response_flags = fields.response_flags;
        self.error_source = fields.error_source;
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
    pub fn billing(mut self, fields: AccessBillingFields) -> Self {
        self.billing = fields;
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
            client_request_id: self.client_request_id,
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
            api_key_id: self.api_key_id.unwrap_or_default(),
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
            retry_count: u32::try_from(self.retry_reasons.len()).unwrap_or(u32::MAX),
            retry_reasons: self.retry_reasons.join(","),
            upstream_request_id: self.upstream_request_id.unwrap_or_default(),
            error_code: self.error_code,
            error_stage: self.error_stage,
            response_flags: self.response_flags,
            error_source: self.error_source,
            upstream_mode: self.upstream_mode,
            contract_id: self.contract_id,
            error_class: self.error_class,
            event_id: self.billing.event_id,
            billable: self.billing.billable,
            price: self.billing.price,
            billing_code: self.billing.billing_code,
            cost: self.billing.cost,
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
    /// W3C `trace-flags` 的 sampled 位：本段 span 的真实采样决定（`ParentBased` 跟随入站父节点）。
    pub sampled: bool,
    /// 入站 `X-Request-Id` 不合规被替换时的原值（截断），否则为空；只在入口服务有值。
    pub client_request_id: Arc<str>,
}

impl TraceContext {
    /// 以 `span` 的 OpenTelemetry 上下文作为本段链路 ID；`parent_span_id` 是远端父 span（无则为空）。
    ///
    /// 仅当 OpenTelemetry 层未记录该 span（未启用 OTLP 导出）时，才退回随机 ID 以保证日志仍可关联，
    /// 此时没有任何 span 会被导出，sampled 为 `false`。
    #[must_use]
    pub fn for_span(request_id: Arc<str>, span: &tracing::Span, parent_span_id: Arc<str>) -> Self {
        let (trace_id, span_id, sampled) = match crate::trace_propagation::span_context_of(span) {
            Some(span_context) => (
                span_context.trace_id().to_string(),
                span_context.span_id().to_string(),
                span_context.is_sampled(),
            ),
            None => (
                Uuid::new_v4().simple().to_string(),
                current_span_id(),
                false,
            ),
        };
        Self {
            request_id,
            trace_id: trace_id.into(),
            span_id: span_id.into(),
            parent_span_id,
            sampled,
            client_request_id: Arc::from(""),
        }
    }

    /// 当前 tracing span 的链路上下文，配一个新的 `request_id`。
    #[must_use]
    pub fn current() -> Self {
        Self::for_span(new_request_id(), &tracing::Span::current(), Arc::from(""))
    }

    /// 记下被替换的入站原始请求 ID（见 [`inbound_request_id`]）。
    #[must_use]
    pub fn with_client_request_id(mut self, client_request_id: Arc<str>) -> Self {
        self.client_request_id = client_request_id;
        self
    }

    /// 以本上下文为父节点的远端 OpenTelemetry span 上下文；ID 非法时返回 `None`。
    #[must_use]
    pub fn span_context(&self) -> Option<opentelemetry::trace::SpanContext> {
        crate::trace_propagation::remote_span_context(&self.trace_id, &self.span_id, self.sampled)
    }

    /// W3C `traceparent`；trace-flags 如实反映 sampled 位（`01` / `00`）。
    #[must_use]
    pub fn traceparent_value(&self) -> String {
        let flags = if self.sampled { "01" } else { "00" };
        format!("00-{}-{}-{flags}", self.trace_id, self.span_id)
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

tokio::task_local! {
    static REQUEST_TRACE: TraceContext;
}

/// 在 `fut` 内把 `trace` 设为本请求的链路上下文（入口中间件调用），供错误响应与访问日志取真实请求 ID。
pub async fn scope_request_trace<F: std::future::Future>(trace: TraceContext, fut: F) -> F::Output {
    REQUEST_TRACE.scope(trace, fut).await
}

/// 当前任务所在请求的链路上下文；不在请求作用域内（后台任务等）时为 `None`。
#[must_use]
pub fn current_request_trace() -> Option<TraceContext> {
    REQUEST_TRACE.try_with(Clone::clone).ok()
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

impl<S: std::hash::BuildHasher> TraceHeaderSource for HashMap<String, String, S> {
    fn header_value(&self, name: &str) -> Option<String> {
        self.get(name)
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }
}

/// 请求 ID 最大长度；与字符集 `[A-Za-z0-9._-]` 一起构成对外协议（`docs/tenant-integration-protocol.md` §2.1）。
pub const REQUEST_ID_MAX_LEN: usize = 128;
/// 被替换的入站原始请求 ID 最多保留的字节数。
pub const CLIENT_REQUEST_ID_MAX_LEN: usize = 256;

/// 是否为合规请求 ID：`[A-Za-z0-9._-]{1,128}`。
#[must_use]
pub fn is_valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= REQUEST_ID_MAX_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// 本服务新签发的请求流水号：`UUIDv7` 的带连字符标准字符串，按生成时间有序。
#[must_use]
fn new_request_id() -> Arc<str> {
    Uuid::now_v7().to_string().into()
}

/// 合规则原样使用，否则（含空值）生成新的 `UUIDv7`。用于内部跳转收到的请求 ID。
#[must_use]
pub fn request_id_or_new(raw: &str) -> Arc<str> {
    let trimmed = raw.trim();
    if is_valid_request_id(trimmed) {
        trimmed.into()
    } else {
        new_request_id()
    }
}

/// 入口对 `X-Request-Id` 的判定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundRequestId {
    /// 本次请求的请求 ID：合规的入站值原样保留，否则为新生成的 `UUIDv7`。
    pub request_id: Arc<str>,
    /// 入站值不合规时的原值（按字节截断到 [`CLIENT_REQUEST_ID_MAX_LEN`]），否则为空。
    pub client_request_id: Arc<str>,
}

/// 入口判定 `X-Request-Id`：合规（`[A-Za-z0-9._-]{1,128}`）原样保留；缺失则新生成 `UUIDv7`；
/// 不合规则替换为新的 `UUIDv7`，原值（含非 ASCII 字节，按 UTF-8 宽松解码）截断后另存。
#[must_use]
pub fn inbound_request_id(headers: &HeaderMap) -> InboundRequestId {
    let raw = headers
        .get("x-request-id")
        .map(|value| String::from_utf8_lossy(value.as_bytes()).trim().to_string())
        .filter(|value| !value.is_empty());
    match raw {
        Some(value) if is_valid_request_id(&value) => InboundRequestId {
            request_id: value.into(),
            client_request_id: Arc::from(""),
        },
        Some(value) => InboundRequestId {
            request_id: new_request_id(),
            client_request_id: truncate_utf8(&value, CLIENT_REQUEST_ID_MAX_LEN).into(),
        },
        None => InboundRequestId {
            request_id: new_request_id(),
            client_request_id: Arc::from(""),
        },
    }
}

fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

/// 内部跳转读取 `x-request-id`（入口中间件已写回合规值）；缺失或不合规时新生成。
#[must_use]
pub fn request_id_from_headers<H: TraceHeaderSource>(headers: &H) -> Arc<str> {
    headers
        .header_value("x-request-id")
        .map_or_else(new_request_id, |value| request_id_or_new(&value))
}

/// 读取入口中间件写回请求头的链路上下文（`traceparent` 即入口 span）。
///
/// 头里没有合法 `traceparent` 时，退回当前 tracing span 的 OpenTelemetry 上下文。
#[must_use]
pub fn parse_trace_context<H: TraceHeaderSource>(headers: &H) -> TraceContext {
    let request_id = request_id_from_headers(headers);
    let parent_span_id: Arc<str> = headers
        .header_value("x-parent-span-id")
        .map_or_else(|| Arc::from(""), Arc::<str>::from);
    match headers
        .header_value("traceparent")
        .as_deref()
        .and_then(parse_traceparent_value)
    {
        Some((trace_id, span_id, sampled)) => TraceContext {
            request_id,
            trace_id: trace_id.into(),
            span_id: span_id.into(),
            parent_span_id,
            sampled,
            client_request_id: Arc::from(""),
        },
        None => TraceContext::for_span(request_id, &tracing::Span::current(), parent_span_id),
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

/// 访问日志的错误来源（对标 Kong `source`、Apigee `policy_error` / `target_error`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSource {
    /// 成功（或 3xx），不记来源。
    None,
    /// 请求本身的问题：参数非法、凭据无效 / 无权限、路由不存在、请求体过大、幂等冲突。
    Client,
    /// 平台策略或平台自身：限流、余额不足、过载、熔断、配置错误、内部错误。
    Gateway,
    /// 上游：连接失败、超时、上游返回错误或拒绝凭据。
    Upstream,
}

impl ErrorSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Client => "client",
            Self::Gateway => "gateway",
            Self::Upstream => "upstream",
        }
    }
}

/// 一次请求结果的分类：Envoy 标准 response flags 缩写 + 错误来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutcomeClass {
    /// Envoy `%RESPONSE_FLAGS%` 缩写（`UAEX` / `RL` / `NR` / `UT` / `UF` / `UH` / `UO` / `DC`），
    /// 没有对应标志时为 `-`。
    pub response_flags: &'static str,
    pub error_source: ErrorSource,
}

impl OutcomeClass {
    #[must_use]
    pub const fn new(response_flags: &'static str, error_source: ErrorSource) -> Self {
        Self {
            response_flags,
            error_source,
        }
    }

    pub const OK: Self = Self::new("-", ErrorSource::None);
}

/// 按业务错误码、出错阶段和 HTTP 状态把一次请求归类为 Envoy 标准 response flag 与错误来源。
///
/// 优先看错误码（网关 `E4xxx` / `E5xxx` 与 bypass 的 `UPSTREAM_*` / `CONFIG_ERROR` 等），
/// 再看阶段，最后按状态码兜底；无法对应 Envoy 标志的记 `-`，不再一律兜底成 `UF`。
/// 同一错误码对应多种原因时（如 `E5020` 既可能是连接失败也可能是熔断），由产生错误的一方
/// 直接给出 [`OutcomeClass`]，不走本函数。
#[must_use]
pub fn classify_outcome(error_code: &str, stage: &str, status_code: u16) -> OutcomeClass {
    use ErrorSource::{Client, Gateway, Upstream};
    let code = error_code.trim().to_ascii_uppercase();
    if code.is_empty() && status_code < 400 {
        return OutcomeClass::OK;
    }
    if status_code == 408 {
        // 客户端发送请求超时（网关复用 E5040 错误码），不是上游超时。
        return OutcomeClass::new("-", Client);
    }
    let stage = stage.trim().to_ascii_lowercase();
    let by_code = match code.as_str() {
        "E4010" | "E4030" | "E4031" => Some(OutcomeClass::new("UAEX", Client)),
        "E4040" => Some(OutcomeClass::new("NR", Client)),
        "E4000" | "E4090" | "E4130" => Some(OutcomeClass::new("-", Client)),
        "E4020" => Some(OutcomeClass::new("-", Gateway)),
        "E4290" => Some(OutcomeClass::new("RL", Gateway)),
        "E5000" | "E5030" | "IDEMPOTENCY_UNAVAILABLE" | "CONFIG_ERROR" | "SSRF_BLOCKED" => {
            Some(OutcomeClass::new("-", Gateway))
        }
        "E5020" | "UPSTREAM_ERROR" => Some(OutcomeClass::new("UF", Upstream)),
        "E5021" | "AUTH_ERROR" => Some(OutcomeClass::new("-", Upstream)),
        "E5040" | "UPSTREAM_TIMEOUT" => Some(OutcomeClass::new("UT", Upstream)),
        _ => None,
    };
    if let Some(class) = by_code {
        return class;
    }
    if code.contains("TIMEOUT") || stage.contains("timeout") {
        return OutcomeClass::new("UT", Upstream);
    }
    if code.contains("RATE_LIMIT") || stage == "ratelimit" {
        return OutcomeClass::new("RL", Gateway);
    }
    if stage == "auth" {
        return OutcomeClass::new("UAEX", Client);
    }
    if stage == "upstream" || stage == "cbpro_fallback" {
        // 上游返回了响应（非 2xx 或业务错误）：Envoy 不打标志，状态码本身说明问题。
        return OutcomeClass::new("-", Upstream);
    }
    match status_code {
        401 | 403 => OutcomeClass::new("UAEX", Client),
        404 => OutcomeClass::new("NR", Client),
        429 => OutcomeClass::new("RL", Gateway),
        402 => OutcomeClass::new("-", Gateway),
        499 => OutcomeClass::new("DC", Client),
        400..=499 => OutcomeClass::new("-", Client),
        502 => OutcomeClass::new("UF", Upstream),
        504 => OutcomeClass::new("UT", Upstream),
        _ => OutcomeClass::new("-", Gateway),
    }
}

pub fn emit_access_log(target: &str, event: &AccessLogEvent) {
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
            client_request_id = %$event.client_request_id,
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
            api_key_id = %$event.api_key_id,
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
            retry_reasons = %$event.retry_reasons,
            upstream_request_id = %$event.upstream_request_id,
            error_code = %$event.error_code,
            error_stage = %$event.error_stage,
            response_flags = %$event.response_flags,
            error_source = %$event.error_source,
            upstream_mode = %$event.upstream_mode,
            contract_id = %$event.contract_id,
            error_class = %$event.error_class,
            event_id = %$event.event_id,
            billable = $event.billable,
            price = %$event.price,
            billing_code = $event.billing_code,
            cost = %$event.cost,
        );
    };
}

fn emit_access_log_gateway(event: &AccessLogEvent) {
    emit_access_log_for_target!("gateway.access", event);
}

fn emit_access_log_billing(event: &AccessLogEvent) {
    emit_access_log_for_target!("billing.access", event);
}

fn emit_access_log_bypass(event: &AccessLogEvent) {
    emit_access_log_for_target!("mazbri-bypass.access", event);
}

fn emit_access_log_generic(target: &str, event: &AccessLogEvent) {
    info!(
        target: "access",
        log_target = %target,
        message = %event.message,
        service = %event.service,
        host = %event.host,
        instance_id = %event.instance_id,
        request_id = %event.request_id,
        client_request_id = %event.client_request_id,
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
        api_key_id = %event.api_key_id,
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
        retry_reasons = %event.retry_reasons,
        upstream_request_id = %event.upstream_request_id,
        error_code = %event.error_code,
        error_stage = %event.error_stage,
        response_flags = %event.response_flags,
        error_source = %event.error_source,
        upstream_mode = %event.upstream_mode,
        contract_id = %event.contract_id,
        error_class = %event.error_class,
        event_id = %event.event_id,
        billable = event.billable,
        price = %event.price,
        billing_code = event.billing_code,
        cost = %event.cost,
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

fn parse_traceparent_value(value: &str) -> Option<(String, String, bool)> {
    crate::trace_spans::span_context_from_traceparent(value).map(|span_context| {
        (
            span_context.trace_id().to_string(),
            span_context.span_id().to_string(),
            span_context.is_sampled(),
        )
    })
}

#[cfg(test)]
mod request_id_tests {
    use super::{
        CLIENT_REQUEST_ID_MAX_LEN, TraceContext, current_request_trace, inbound_request_id,
        is_valid_request_id, parse_trace_context, request_id_or_new, scope_request_trace,
    };
    use axum::http::{HeaderMap, HeaderValue};
    use std::collections::HashMap;
    use std::sync::Arc;
    use uuid::Uuid;

    fn assert_generated_v7(id: &str) {
        let uuid = Uuid::parse_str(id).expect("generated request id is a uuid");
        assert_eq!(uuid.get_version_num(), 7);
    }

    #[test]
    fn valid_request_id_charset_and_length() {
        assert!(is_valid_request_id("6e7f448edae94db78e5e9f87f1402eaa"));
        assert!(is_valid_request_id("probe-0193a1b2.c3_d4"));
        assert!(is_valid_request_id(&"a".repeat(128)));
        assert!(!is_valid_request_id(&"a".repeat(129)));
        assert!(!is_valid_request_id(""));
        assert!(!is_valid_request_id("has space"));
        assert!(!is_valid_request_id("中文"));
        assert!(!is_valid_request_id("a/b"));
    }

    #[test]
    fn conforming_inbound_id_is_kept_verbatim() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-request-id",
            HeaderValue::from_static("6E7F448EDAE94DB78E5E9F87F1402EAA"),
        );
        let inbound = inbound_request_id(&headers);
        assert_eq!(
            inbound.request_id.as_ref(),
            "6E7F448EDAE94DB78E5E9F87F1402EAA"
        );
        assert!(inbound.client_request_id.is_empty());
    }

    #[test]
    fn non_conforming_inbound_id_is_replaced_and_original_kept() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", HeaderValue::from_static("order 42/abc"));
        let inbound = inbound_request_id(&headers);
        assert_generated_v7(&inbound.request_id);
        assert_eq!(inbound.client_request_id.as_ref(), "order 42/abc");

        let long = "x".repeat(300);
        headers.insert("x-request-id", HeaderValue::from_str(&long).expect("ascii"));
        let inbound = inbound_request_id(&headers);
        assert_generated_v7(&inbound.request_id);
        assert_eq!(inbound.client_request_id.len(), CLIENT_REQUEST_ID_MAX_LEN);

        headers.insert(
            "x-request-id",
            HeaderValue::from_bytes("订单-1".as_bytes()).expect("opaque bytes"),
        );
        let inbound = inbound_request_id(&headers);
        assert_eq!(inbound.client_request_id.as_ref(), "订单-1");
    }

    #[test]
    fn missing_inbound_id_generates_uuid_without_client_id() {
        let inbound = inbound_request_id(&HeaderMap::new());
        assert_generated_v7(&inbound.request_id);
        assert!(inbound.client_request_id.is_empty());
    }

    #[test]
    fn internal_hop_id_kept_or_regenerated() {
        assert_eq!(request_id_or_new("abc-1").as_ref(), "abc-1");
        assert_generated_v7(&request_id_or_new("bad id"));
        assert_generated_v7(&request_id_or_new(""));
    }

    #[test]
    fn generated_request_ids_are_uuid_v7() {
        assert_generated_v7(&inbound_request_id(&HeaderMap::new()).request_id);
        assert_generated_v7(&request_id_or_new("not valid"));
        assert_generated_v7(&TraceContext::current().request_id);
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", HeaderValue::from_static("bad/id"));
        assert_generated_v7(&inbound_request_id(&headers).request_id);
    }

    #[test]
    fn current_without_otel_span_still_has_w3c_ids() {
        let ctx = TraceContext::current();
        assert_generated_v7(&ctx.request_id);
        assert_eq!(ctx.request_id.len(), 36);
        assert_eq!(ctx.trace_id.len(), 32);
        assert_eq!(ctx.span_id.len(), 16);
        assert!(
            !ctx.sampled,
            "nothing exports without an OpenTelemetry span"
        );
        assert!(ctx.span_context().is_some());
    }

    #[test]
    fn parse_reads_traceparent_flags_and_keeps_request_id() {
        let mut headers = HashMap::new();
        headers.insert(
            "x-request-id".to_string(),
            "6e7f448edae94db78e5e9f87f1402eaa".to_string(),
        );
        headers.insert(
            "traceparent".to_string(),
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00".to_string(),
        );
        headers.insert(
            "x-parent-span-id".to_string(),
            "b7ad6b7169203331".to_string(),
        );
        let ctx = parse_trace_context(&headers);
        assert_eq!(ctx.request_id.as_ref(), "6e7f448edae94db78e5e9f87f1402eaa");
        assert_eq!(ctx.trace_id.as_ref(), "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.span_id.as_ref(), "00f067aa0ba902b7");
        assert_eq!(ctx.parent_span_id.as_ref(), "b7ad6b7169203331");
        assert!(!ctx.sampled);
        assert_eq!(
            ctx.traceparent_value(),
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00"
        );
    }

    #[test]
    fn parse_ignores_malformed_traceparent() {
        let mut headers = HashMap::new();
        headers.insert("traceparent".to_string(), "00-abc-def-01".to_string());
        let ctx = parse_trace_context(&headers);
        assert_eq!(ctx.trace_id.len(), 32);
        assert_ne!(ctx.trace_id.as_ref(), "abc");
    }

    fn fixed_context(sampled: bool) -> TraceContext {
        TraceContext {
            request_id: Arc::from("r"),
            trace_id: Arc::from("4bf92f3577b34da6a3ce929d0e0e4736"),
            span_id: Arc::from("00f067aa0ba902b7"),
            parent_span_id: Arc::from(""),
            sampled,
            client_request_id: Arc::from(""),
        }
    }

    #[test]
    fn trace_context_span_context_round_trip() {
        let ctx = fixed_context(true);
        let span_context = ctx.span_context().expect("valid ids");
        assert_eq!(span_context.trace_id().to_string(), ctx.trace_id.as_ref());
        assert_eq!(span_context.span_id().to_string(), ctx.span_id.as_ref());
        assert!(span_context.is_sampled());
        assert_eq!(
            ctx.traceparent_value(),
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        );
        let unsampled = fixed_context(false);
        assert!(!unsampled.span_context().expect("valid ids").is_sampled());
        assert!(unsampled.traceparent_value().ends_with("-00"));
    }

    #[tokio::test]
    async fn request_trace_scope_is_visible_inside_only() {
        assert!(current_request_trace().is_none());
        let ctx = fixed_context(true);
        let seen = scope_request_trace(ctx.clone(), async { current_request_trace() }).await;
        assert_eq!(seen, Some(ctx));
        assert!(current_request_trace().is_none());
    }
}

#[cfg(test)]
mod classify_outcome_tests {
    use super::{ErrorSource, OutcomeClass, classify_outcome};

    fn class(code: &str, stage: &str, status: u16) -> (&'static str, &'static str) {
        let c = classify_outcome(code, stage, status);
        (c.response_flags, c.error_source.as_str())
    }

    #[test]
    fn success_has_no_flag_and_no_source() {
        assert_eq!(classify_outcome("", "", 200), OutcomeClass::OK);
        assert_eq!(class("", "", 304), ("-", ""));
    }

    #[test]
    fn gateway_rejections_use_envoy_flags() {
        assert_eq!(class("E4010", "auth", 401), ("UAEX", "client"));
        assert_eq!(class("E4031", "auth", 403), ("UAEX", "client"));
        assert_eq!(class("E4040", "routing", 404), ("NR", "client"));
        assert_eq!(class("E4290", "ratelimit", 429), ("RL", "gateway"));
        assert_eq!(class("E4020", "balance", 402), ("-", "gateway"));
        assert_eq!(class("E4000", "request", 400), ("-", "client"));
        assert_eq!(class("E5000", "gateway", 500), ("-", "gateway"));
    }

    #[test]
    fn upstream_failures_are_attributed_to_upstream() {
        assert_eq!(class("E5020", "upstream", 502), ("UF", "upstream"));
        assert_eq!(class("E5040", "upstream", 504), ("UT", "upstream"));
        assert_eq!(class("E5021", "upstream", 502), ("-", "upstream"));
        assert_eq!(
            class("UPSTREAM_TIMEOUT", "upstream", 504),
            ("UT", "upstream")
        );
        assert_eq!(class("UPSTREAM_BIZ", "upstream", 400), ("-", "upstream"));
    }

    #[test]
    fn unknown_errors_no_longer_default_to_uf() {
        assert_eq!(class("SOMETHING", "", 500), ("-", "gateway"));
        assert_eq!(class("SOMETHING", "", 418), ("-", "client"));
        assert_eq!(class("", "", 408), ("-", "client"));
        assert_eq!(class("CONFIG_ERROR", "upstream", 502), ("-", "gateway"));
        assert_eq!(ErrorSource::None.as_str(), "");
    }
}
