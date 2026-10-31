//! 主动埋点：按已测耗时补记已完成的 span、依赖客户端 span、span event 与 link。
//!
//! 网关阶段、Redis、ClickHouse、GreptimeDB 等调用点本身已经测量了耗时，这里用同一份耗时
//! 补记一个已结束的 OpenTelemetry span（开始时间 = 结束时间 − 耗时），保证瀑布图上的阶段耗时
//! 与 `request_stage_logs` 的 `stage_ms` 完全一致。PG 查询由 [`DependencySpanLayer`] 从 sqlx
//! 的 `sqlx::query` 事件补记，不记录 SQL 文本与参数值，只记录操作与表名。

pub use opentelemetry::Context as OtelContext;
use opentelemetry::KeyValue;
use opentelemetry::trace::{
    Span as _, SpanContext, SpanId, SpanKind, Status, TraceContextExt, TraceFlags, TraceId,
    TraceState, Tracer as _,
};
use opentelemetry_sdk::trace::Tracer;
use std::borrow::Cow;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};
use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing_opentelemetry::{OpenTelemetrySpanExt, OtelData, PreSampledTracer};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context as LayerContext;
use tracing_subscriber::registry::LookupSpan;

/// span event 专用 target：只有这个 target 的事件会被 OpenTelemetry 层记成当前 span 的 event。
pub const SPAN_EVENT_TARGET: &str = "mazbri::span_event";

const SQLX_QUERY_TARGET: &str = "sqlx::query";

static TRACER: OnceLock<Tracer> = OnceLock::new();

/// 记录导出用的 tracer；由追踪初始化在 OTLP 启用时调用一次。
pub(crate) fn install_tracer(tracer: Tracer) {
    let _ = TRACER.set(tracer);
}

/// 一个已经结束、耗时已知的 span。
#[derive(Debug, Clone)]
pub struct CompletedSpan {
    name: Cow<'static, str>,
    kind: SpanKind,
    duration: Duration,
    attributes: Vec<KeyValue>,
    error: Option<String>,
}

impl CompletedSpan {
    #[must_use]
    pub fn new(name: impl Into<Cow<'static, str>>, duration: Duration) -> Self {
        Self {
            name: name.into(),
            kind: SpanKind::Internal,
            duration,
            attributes: Vec::new(),
            error: None,
        }
    }

    /// 以毫秒耗时构造，与阶段日志的 `stage_ms` 同源。
    #[must_use]
    pub fn from_millis(name: impl Into<Cow<'static, str>>, millis: u64) -> Self {
        Self::new(name, Duration::from_millis(millis))
    }

    #[must_use]
    pub fn client(mut self) -> Self {
        self.kind = SpanKind::Client;
        self
    }

    #[must_use]
    pub fn attr(mut self, key: &'static str, value: impl Into<opentelemetry::Value>) -> Self {
        self.attributes.push(KeyValue::new(key, value));
        self
    }

    /// 标记为出错；`message` 写入 span status。
    #[must_use]
    pub fn error(mut self, message: impl Into<String>) -> Self {
        self.error = Some(message.into());
        self
    }

    /// 以 `parent` 为父节点补记；未启用导出或父上下文无效时不记录，返回 `None`。
    pub fn record(self, parent: &OtelContext) -> Option<SpanContext> {
        let tracer = TRACER.get()?;
        if !parent.span().span_context().is_valid() {
            return None;
        }
        let end = SystemTime::now();
        let start = end.checked_sub(self.duration).unwrap_or(end);
        let mut span = tracer
            .span_builder(self.name)
            .with_kind(self.kind)
            .with_start_time(start)
            .with_attributes(self.attributes)
            .start_with_context(tracer, parent);
        if let Some(message) = self.error {
            span.set_status(Status::error(message));
        }
        let span_context = span.span_context().clone();
        span.end_with_timestamp(end);
        Some(span_context)
    }

    /// 以当前 tracing span 为父节点补记。
    pub fn record_under_current(self) -> Option<SpanContext> {
        TRACER.get()?;
        self.record(&tracing::Span::current().context())
    }
}

/// 当前 tracing span 的 OpenTelemetry 上下文（供异步任务里补记子 span 时作父节点）。
#[must_use]
pub fn current_context() -> OtelContext {
    tracing::Span::current().context()
}

/// `00-<trace>-<span>-<flags>` 形式的 W3C `traceparent`。
#[must_use]
pub fn traceparent_of(span_context: &SpanContext) -> String {
    format!(
        "00-{}-{}-{:02x}",
        span_context.trace_id(),
        span_context.span_id(),
        span_context.trace_flags().to_u8()
    )
}

/// 解析 W3C `traceparent`；格式或 ID 非法时返回 `None`。
#[must_use]
pub fn span_context_from_traceparent(value: &str) -> Option<SpanContext> {
    let mut parts = value.trim().split('-');
    let version = parts.next()?;
    let trace_id = TraceId::from_hex(parts.next()?).ok()?;
    let span_id = SpanId::from_hex(parts.next()?).ok()?;
    let flags = u8::from_str_radix(parts.next()?, 16).ok()?;
    if version != "00" || parts.next().is_some() {
        return None;
    }
    let span_context = SpanContext::new(
        trace_id,
        span_id,
        TraceFlags::new(flags),
        true,
        TraceState::default(),
    );
    span_context.is_valid().then_some(span_context)
}

/// 给 `span` 追加指向各 `traceparent` 的 link（非法值跳过），返回追加的个数。
pub fn add_links<'a>(
    span: &tracing::Span,
    traceparents: impl IntoIterator<Item = &'a str>,
) -> usize {
    let mut added = 0;
    for value in traceparents {
        if let Some(span_context) = span_context_from_traceparent(value) {
            span.add_link(span_context);
            added += 1;
        }
    }
    added
}

/// 批量日志用：把各条 `traceparent` 的 `trace_id` 去重后以逗号拼接，
/// 使批量 span（无父、仅 link）的完成日志也能按请求 `trace_id` 检索到。
#[must_use]
pub fn linked_trace_ids<'a>(traceparents: impl IntoIterator<Item = &'a str>) -> String {
    let mut ids: Vec<String> = traceparents
        .into_iter()
        .filter_map(span_context_from_traceparent)
        .map(|span_context| span_context.trace_id().to_string())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids.join(",")
}

/// 由 SQL 文本取 `(操作, 表名)`；只看关键字，不保留任何字面值。
#[must_use]
pub fn sql_operation_and_table(sql: &str) -> (String, Option<String>) {
    let words: Vec<&str> = sql.split_whitespace().collect();
    let Some(first) = words.first() else {
        return (String::new(), None);
    };
    let operation = first.to_ascii_uppercase();
    let after = |keyword: &str| {
        words
            .iter()
            .position(|word| word.eq_ignore_ascii_case(keyword))
            .and_then(|index| words.get(index + 1))
            .and_then(|word| table_name(word))
    };
    let table = match operation.as_str() {
        "INSERT" => after("INTO"),
        "UPDATE" => words.get(1).and_then(|word| table_name(word)),
        _ => after("FROM"),
    };
    (operation, table)
}

fn table_name(word: &str) -> Option<String> {
    let name: String = word
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '"' | '`'))
        .filter(|c| !matches!(c, '"' | '`'))
        .collect();
    (!name.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_'))
    .then_some(name)
}

/// 事件所在 span 链上第一个被 OpenTelemetry 层记录的 span 的上下文。
fn event_parent_context<S>(
    ctx: &LayerContext<'_, S>,
    event: &tracing::Event<'_>,
    tracer: &Tracer,
) -> Option<OtelContext>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let scope = ctx.event_scope(event)?;
    for span in scope {
        let mut extensions = span.extensions_mut();
        if let Some(data) = extensions.get_mut::<OtelData>() {
            return Some(tracer.sampled_context(data));
        }
    }
    None
}

#[derive(Default)]
struct SqlxQueryVisitor {
    summary: String,
    statement: String,
    elapsed_secs: Option<f64>,
}

impl Visit for SqlxQueryVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "summary" => self.summary = value.to_string(),
            "db.statement" => self.statement = value.to_string(),
            _ => {}
        }
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "elapsed_secs" {
            self.elapsed_secs = Some(value);
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}

/// 依赖 span 层：
/// - sqlx 每条语句结束时的 `sqlx::query` 事件 → `db.query` 子 span（`db.system`、`db.operation`、`db.sql.table`）；
/// - tarpc client/server span → 补 `rpc.system=tarpc`、`rpc.service`、`rpc.method` 属性。
///
/// 必须叠在 OpenTelemetry 层之后，`on_new_span` 才能拿到该层创建的 [`OtelData`]。
pub struct DependencySpanLayer;

impl DependencySpanLayer {
    /// 本层只需要的元数据：sqlx 语句事件与 tarpc span。
    #[must_use]
    pub fn interested(metadata: &tracing::Metadata<'_>) -> bool {
        if metadata.is_event() {
            metadata.target() == SQLX_QUERY_TARGET
        } else {
            is_tarpc_span_target(metadata.target())
        }
    }
}

fn is_tarpc_span_target(target: &str) -> bool {
    target.starts_with("tarpc::client") || target.starts_with("tarpc::server")
}

impl<S> Layer<S> for DependencySpanLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: LayerContext<'_, S>,
    ) {
        let metadata = attrs.metadata();
        if !is_tarpc_span_target(metadata.target()) {
            return;
        }
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut extensions = span.extensions_mut();
        let Some(data) = extensions.get_mut::<OtelData>() else {
            return;
        };
        let name = data.builder.name.to_string();
        let (service, method) = name.split_once('.').unwrap_or(("", name.as_str()));
        let attributes = data.builder.attributes.get_or_insert_with(Vec::new);
        attributes.push(KeyValue::new("rpc.system", "tarpc"));
        attributes.push(KeyValue::new("rpc.service", service.to_string()));
        attributes.push(KeyValue::new("rpc.method", method.to_string()));
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: LayerContext<'_, S>) {
        if event.metadata().target() != SQLX_QUERY_TARGET {
            return;
        }
        let Some(tracer) = TRACER.get() else {
            return;
        };
        let mut visitor = SqlxQueryVisitor::default();
        event.record(&mut visitor);
        let Some(elapsed_secs) = visitor.elapsed_secs else {
            return;
        };
        let Some(parent) = event_parent_context(&ctx, event, tracer) else {
            return;
        };
        let sql = if visitor.statement.trim().is_empty() {
            visitor.summary.as_str()
        } else {
            visitor.statement.as_str()
        };
        let (operation, table) = sql_operation_and_table(sql);
        let mut span =
            CompletedSpan::new("db.query", Duration::from_secs_f64(elapsed_secs.max(0.0)))
                .client()
                .attr("db.system", "postgresql")
                .attr("db.operation", operation);
        if let Some(table) = table {
            span = span.attr("db.sql.table", table);
        }
        span.record(&parent);
    }
}

/// 事件链上 OpenTelemetry span 的 `(trace_id, span_id)`，供日志输出关联字段。
pub(crate) fn otel_ids_of<'a, R>(
    scope: impl Iterator<Item = tracing_subscriber::registry::SpanRef<'a, R>>,
) -> Option<(TraceId, SpanId)>
where
    R: LookupSpan<'a> + 'a,
{
    for span in scope {
        let extensions = span.extensions();
        if let Some(data) = extensions.get::<OtelData>() {
            let trace_id = data
                .builder
                .trace_id
                .unwrap_or_else(|| data.parent_cx.span().span_context().trace_id());
            let span_id = data.builder.span_id?;
            return (trace_id != TraceId::INVALID && span_id != SpanId::INVALID)
                .then_some((trace_id, span_id));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_trace_ids_dedups_and_skips_invalid() {
        let a = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let b = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        assert_eq!(
            linked_trace_ids([a, "garbage", b, a]),
            "0af7651916cd43dd8448eb211c80319c,4bf92f3577b34da6a3ce929d0e0e4736"
        );
    }

    #[test]
    fn sql_summary_takes_operation_and_table_only() {
        assert_eq!(
            sql_operation_and_table("SELECT id, name FROM usage_outbox WHERE id = $1"),
            ("SELECT".to_string(), Some("usage_outbox".to_string()))
        );
        assert_eq!(
            sql_operation_and_table("\n\nINSERT INTO billing.usage_records (a) VALUES ($1)\n"),
            (
                "INSERT".to_string(),
                Some("billing.usage_records".to_string())
            )
        );
        assert_eq!(
            sql_operation_and_table("UPDATE usage_charge_queue SET status = 'x'"),
            ("UPDATE".to_string(), Some("usage_charge_queue".to_string()))
        );
        assert_eq!(
            sql_operation_and_table("SELECT 1"),
            ("SELECT".to_string(), None)
        );
        assert_eq!(
            sql_operation_and_table("SELECT * FROM (SELECT 1) t"),
            ("SELECT".to_string(), None)
        );
    }

    #[test]
    fn traceparent_round_trip() {
        let value = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let span_context = span_context_from_traceparent(value).expect("valid traceparent");
        assert_eq!(traceparent_of(&span_context), value);
        assert!(span_context_from_traceparent("00-xyz-00f067aa0ba902b7-01").is_none());
        assert!(span_context_from_traceparent("").is_none());
    }

    #[test]
    fn completed_span_without_tracer_is_noop() {
        assert!(
            CompletedSpan::from_millis("transport", 3)
                .record_under_current()
                .is_none()
        );
    }
}
