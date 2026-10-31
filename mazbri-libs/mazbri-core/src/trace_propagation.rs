//! W3C Trace Context 透传：HTTP 头、gRPC 元数据与 tarpc 上下文之间的互转。
//!
//! 进程内的 trace/span ID 一律取自 OpenTelemetry 当前 span；跨进程只传 W3C
//! `traceparent`/`tracestate`（HTTP/gRPC）或 tarpc 自带的 `trace::Context`。

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use opentelemetry::Context as OtelContext;
use opentelemetry::propagation::{Extractor, Injector};
use opentelemetry::trace::{SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use std::collections::HashMap;
use tracing_opentelemetry::OpenTelemetrySpanExt;

const TRACEPARENT: &str = "traceparent";
const TRACESTATE: &str = "tracestate";

/// 注册全局 W3C Trace Context propagator；由日志/追踪初始化统一调用。
pub fn install_global_propagator() {
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
}

struct HeaderMapCarrier<'a>(&'a HeaderMap);

impl Extractor for HeaderMapCarrier<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(HeaderName::as_str).collect()
    }
}

struct HeaderMapInjector<'a>(&'a mut HeaderMap);

impl Injector for HeaderMapInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            self.0.insert(name, value);
        }
    }
}

struct StringMapCarrier<'a, S>(&'a HashMap<String, String, S>);

impl<S: std::hash::BuildHasher> Extractor for StringMapCarrier<'_, S> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(String::as_str).collect()
    }
}

struct StringMapInjector<'a, S>(&'a mut HashMap<String, String, S>);

impl<S: std::hash::BuildHasher> Injector for StringMapInjector<'_, S> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key.to_string(), value);
    }
}

/// 从 HTTP 头中提取远端父上下文；没有合法 `traceparent` 时返回空上下文。
#[must_use]
pub fn extract_from_headers(headers: &HeaderMap) -> OtelContext {
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderMapCarrier(headers))
    })
}

/// 入站 HTTP 头里的远端父 span（合法 `traceparent`），没有时为 `None`。
#[must_use]
pub fn remote_parent_from_headers(headers: &HeaderMap) -> Option<SpanContext> {
    let context = extract_from_headers(headers);
    let span_context = context.span().span_context().clone();
    span_context.is_valid().then_some(span_context)
}

/// 从字符串头表（大小写不敏感）中提取远端父上下文。
#[must_use]
pub fn extract_from_map<S: std::hash::BuildHasher>(
    headers: &HashMap<String, String, S>,
) -> OtelContext {
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&StringMapCarrier(headers))
    })
}

/// 用当前 span 覆盖写入 `traceparent`/`tracestate`；当前没有有效 span 时只清除旧值。
pub fn inject_current_into_header_map(headers: &mut HeaderMap) {
    headers.remove(TRACEPARENT);
    headers.remove(TRACESTATE);
    let context = tracing::Span::current().context();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&context, &mut HeaderMapInjector(headers));
    });
}

/// [`inject_current_into_header_map`] 的字符串头表版本（大小写不敏感地替换旧值）。
pub fn inject_current_into_map<S: std::hash::BuildHasher>(
    headers: &mut HashMap<String, String, S>,
) {
    headers.retain(|name, _| {
        !name.eq_ignore_ascii_case(TRACEPARENT) && !name.eq_ignore_ascii_case(TRACESTATE)
    });
    let context = tracing::Span::current().context();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&context, &mut StringMapInjector(headers));
    });
}

/// 给定 span 的 OpenTelemetry span 上下文；span 未被 OpenTelemetry 层记录时返回 `None`。
#[must_use]
pub fn span_context_of(span: &tracing::Span) -> Option<SpanContext> {
    let context = span.context();
    let span_context = context.span().span_context().clone();
    span_context.is_valid().then_some(span_context)
}

/// 当前 tracing span 的 OpenTelemetry span 上下文。
#[must_use]
pub fn current_span_context() -> Option<SpanContext> {
    span_context_of(&tracing::Span::current())
}

/// 把远端 span 设为 `span` 的父节点；必须在 `span` 的子 span 创建之前调用。
pub fn set_remote_parent(span: &tracing::Span, parent: SpanContext) {
    if parent.is_valid() {
        span.set_parent(OtelContext::new().with_remote_span_context(parent));
    }
}

/// 由十六进制 trace/span ID 组装远端 span 上下文；任一 ID 非法时返回 `None`。
#[must_use]
pub fn remote_span_context(trace_id: &str, span_id: &str, sampled: bool) -> Option<SpanContext> {
    let trace_id = TraceId::from_hex(trace_id.trim()).ok()?;
    let span_id = SpanId::from_hex(span_id.trim()).ok()?;
    let flags = if sampled {
        TraceFlags::SAMPLED
    } else {
        TraceFlags::default()
    };
    let span_context = SpanContext::new(trace_id, span_id, flags, true, TraceState::default());
    span_context.is_valid().then_some(span_context)
}

/// OpenTelemetry span 上下文 → tarpc 上下文（tarpc 自带的 OpenTelemetry 版本与本仓库不同，只能按 ID 互转）。
#[must_use]
pub fn tarpc_trace_context(span_context: &SpanContext) -> tarpc::trace::Context {
    tarpc::trace::Context {
        trace_id: u128::from_be_bytes(span_context.trace_id().to_bytes()).into(),
        span_id: u64::from_be_bytes(span_context.span_id().to_bytes()).into(),
        sampling_decision: if span_context.is_sampled() {
            tarpc::trace::SamplingDecision::Sampled
        } else {
            tarpc::trace::SamplingDecision::Unsampled
        },
    }
}

/// tarpc 上下文 → 远端 OpenTelemetry span 上下文；ID 为零时返回 `None`。
#[must_use]
pub fn span_context_from_tarpc(context: &tarpc::trace::Context) -> Option<SpanContext> {
    let trace_id = TraceId::from_bytes(u128::from(context.trace_id).to_be_bytes());
    let span_id = SpanId::from_bytes(u64::from(context.span_id).to_be_bytes());
    let flags = match context.sampling_decision {
        tarpc::trace::SamplingDecision::Sampled => TraceFlags::SAMPLED,
        tarpc::trace::SamplingDecision::Unsampled => TraceFlags::default(),
    };
    let span_context = SpanContext::new(trace_id, span_id, flags, true, TraceState::default());
    span_context.is_valid().then_some(span_context)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACEPARENT_VALUE: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    #[test]
    fn extracts_w3c_traceparent_from_header_map() {
        install_global_propagator();
        let mut headers = HeaderMap::new();
        headers.insert(TRACEPARENT, HeaderValue::from_static(TRACEPARENT_VALUE));
        let context = extract_from_headers(&headers);
        let span = context.span();
        let span_context = span.span_context();
        assert!(span_context.is_remote());
        assert_eq!(
            span_context.trace_id().to_string(),
            "4bf92f3577b34da6a3ce929d0e0e4736"
        );
        assert_eq!(span_context.span_id().to_string(), "00f067aa0ba902b7");
        assert!(span_context.is_sampled());
    }

    #[test]
    fn extract_from_map_is_case_insensitive() {
        install_global_propagator();
        let mut headers = HashMap::new();
        headers.insert("TraceParent".to_string(), TRACEPARENT_VALUE.to_string());
        let context = extract_from_map(&headers);
        assert_eq!(
            context.span().span_context().span_id().to_string(),
            "00f067aa0ba902b7"
        );
    }

    #[test]
    fn invalid_traceparent_yields_empty_context() {
        install_global_propagator();
        let mut headers = HeaderMap::new();
        headers.insert(TRACEPARENT, HeaderValue::from_static("00-zz-yy-01"));
        assert!(!extract_from_headers(&headers).has_active_span());
    }

    #[test]
    fn inject_without_span_removes_stale_values() {
        install_global_propagator();
        let mut headers = HashMap::new();
        headers.insert("TraceParent".to_string(), TRACEPARENT_VALUE.to_string());
        headers.insert("x-request-id".to_string(), "r".to_string());
        inject_current_into_map(&mut headers);
        assert!(!headers.keys().any(|k| k.eq_ignore_ascii_case(TRACEPARENT)));
        assert_eq!(headers.get("x-request-id").map(String::as_str), Some("r"));
    }

    #[test]
    fn tarpc_context_round_trips() {
        let original =
            remote_span_context("4bf92f3577b34da6a3ce929d0e0e4736", "00f067aa0ba902b7", true)
                .expect("valid ids");
        let tarpc_context = tarpc_trace_context(&original);
        assert_eq!(
            u128::from(tarpc_context.trace_id),
            0x4bf9_2f35_77b3_4da6_a3ce_929d_0e0e_4736
        );
        assert_eq!(u64::from(tarpc_context.span_id), 0x00f0_67aa_0ba9_02b7);
        let back = span_context_from_tarpc(&tarpc_context).expect("valid context");
        assert_eq!(back.trace_id(), original.trace_id());
        assert_eq!(back.span_id(), original.span_id());
        assert!(back.is_sampled());
    }

    #[test]
    fn zero_tarpc_context_is_rejected() {
        assert!(span_context_from_tarpc(&tarpc::trace::Context::default()).is_none());
        assert!(remote_span_context("0", "0", true).is_none());
    }

    fn with_otel_subscriber(f: impl FnOnce()) {
        use opentelemetry::trace::TracerProvider as _;
        use tracing_subscriber::layer::SubscriberExt;
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
        tracing::subscriber::with_default(subscriber, f);
    }

    #[test]
    fn remote_parent_set_after_creation_takes_over_trace_id() {
        install_global_propagator();
        with_otel_subscriber(|| {
            let parent =
                remote_span_context("4bf92f3577b34da6a3ce929d0e0e4736", "00f067aa0ba902b7", true)
                    .expect("valid ids");
            let span = tracing::info_span!("server");
            set_remote_parent(&span, parent);
            let own = span_context_of(&span).expect("otel span");
            assert_eq!(
                own.trace_id().to_string(),
                "4bf92f3577b34da6a3ce929d0e0e4736"
            );
            assert_ne!(own.span_id().to_string(), "00f067aa0ba902b7");
            let _entered = span.enter();
            let child = tracing::info_span!("child");
            let child_context = span_context_of(&child).expect("otel span");
            assert_eq!(child_context.trace_id(), own.trace_id());
        });
    }

    #[test]
    fn inject_replaces_stale_traceparent_with_current_span() {
        install_global_propagator();
        with_otel_subscriber(|| {
            let span = tracing::info_span!("client");
            let _entered = span.enter();
            let own = current_span_context().expect("otel span");
            let mut headers = HashMap::new();
            headers.insert("TraceParent".to_string(), TRACEPARENT_VALUE.to_string());
            inject_current_into_map(&mut headers);
            assert!(!headers.contains_key("TraceParent"));
            assert_eq!(
                headers.get(TRACEPARENT).map(String::as_str),
                Some(format!("00-{}-{}-01", own.trace_id(), own.span_id()).as_str())
            );
            let mut header_map = HeaderMap::new();
            inject_current_into_header_map(&mut header_map);
            assert_eq!(
                remote_parent_from_headers(&header_map).map(|sc| sc.span_id()),
                Some(own.span_id())
            );
        });
    }
}
