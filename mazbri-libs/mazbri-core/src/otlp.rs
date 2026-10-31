//! Optional OTLP trace export (HTTP/protobuf).
//!
//! Spans are forwarded to `GreptimeDB` through the Vector OTLP receiver:
//! service exporter → Vector `opentelemetry` source (`http://vector:4318`) →
//! Vector `opentelemetry` sink → `GreptimeDB` `/v1/otlp/v1/traces`.
//! Greptime-specific request headers (database, trace pipeline, basic auth)
//! are set by the Vector sink; the service exporter only needs the endpoint.
//!
//! ## Span export filter
//!
//! An unfiltered `tracing-opentelemetry` layer returns `Interest::always`, which
//! enables dependency TRACE spans (notably `h2` / hyper) and floods Greptime with
//! transport noise. Only **business-relevant** spans are exported — see
//! [`should_export_otlp_span`].

use crate::metrics::{SpanDropReason, record_spans_dropped};
use anyhow::{Context, Result};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{Array, KeyValue, StringValue, Value};
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{
    BatchConfigBuilder, BatchSpanProcessor, RandomIdGenerator, Sampler, SdkTracerProvider,
    SpanData, SpanLimits, SpanProcessor, Tracer,
};
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tracing::Level;

/// Per-span caps applied by the SDK.
const MAX_ATTRIBUTES_PER_SPAN: u32 = 64;
const MAX_EVENTS_PER_SPAN: u32 = 32;
const MAX_LINKS_PER_SPAN: u32 = 128;
/// Longest exported string attribute value, in bytes (cut on a char boundary).
pub const MAX_ATTRIBUTE_VALUE_BYTES: usize = 1024;

/// Holds the SDK provider so spans flush on drop/shutdown.
pub struct OtlpGuard {
    provider: SdkTracerProvider,
}

impl Drop for OtlpGuard {
    fn drop(&mut self) {
        if let Err(error) = self.provider.shutdown() {
            // Avoid panicking during drop; stderr is last-resort if subscriber already gone.
            eprintln!("otlp tracer provider shutdown failed: {error}");
        }
    }
}

/// Options for building the OTLP HTTP exporter.
#[derive(Debug, Clone, Copy)]
pub struct OtlpExportOptions<'a> {
    pub endpoint: &'a str,
    pub service_name: &'a str,
    pub sample_ratio: f64,
    pub batch: OtlpBatchOptions,
}

/// Batch span processor sizing (`[tracing]` `otel_max_queue_size` …).
#[derive(Debug, Clone, Copy)]
pub struct OtlpBatchOptions {
    pub max_queue_size: usize,
    pub max_export_batch_size: usize,
    pub scheduled_delay: Duration,
}

/// Gateway stage span names (step 16 `stage_span`).
const GATEWAY_STAGE_NAMES: &[&str] = &[
    "auth",
    "ratelimit",
    "balance_check",
    "idempotency",
    "transport",
    "request_prepare",
    "product_detail_lookup",
    "upstream",
    "response_serialize",
    "response_encrypt",
    "post_response",
    "platform_summary",
];

/// Span name prefixes worth exporting for dependency hops and business logic.
const EXPORTED_NAME_PREFIXES: &[&str] = &[
    "rpc.",
    "db.",
    "redis.",
    "clickhouse.",
    "greptime.",
    "upstream.",
    "billing.",
    "cache.",
    "license.",
];

/// Whether a `tracing` span should be exported via OTLP (allow-list).
///
/// Exported when the level is INFO or above **and** either:
/// - the target is a mazbri crate (`mazbri*`), or
/// - the name is allow-listed: `request` (HTTP root), a gateway stage name,
///   one of [`EXPORTED_NAME_PREFIXES`], or a tarpc RPC name such as
///   `CacheService.get`.
///
/// Everything else (h2 / hyper / rustls internals, DEBUG / TRACE spans,
/// unknown dependency spans) is dropped at the source.
#[must_use]
pub fn should_export_otlp_span(meta: &tracing::Metadata<'_>) -> bool {
    // Level ordering: ERROR < WARN < INFO < DEBUG < TRACE
    if *meta.level() > Level::INFO {
        return false;
    }
    meta.target().starts_with("mazbri") || is_exported_span_name(meta.name())
}

fn is_exported_span_name(name: &str) -> bool {
    name == "request"
        || GATEWAY_STAGE_NAMES.contains(&name)
        || EXPORTED_NAME_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        || is_rpc_style_span_name(name)
}

/// `CacheService.get`, `AuthService.validate_api_key` — tarpc-style RPC span names.
fn is_rpc_style_span_name(name: &str) -> bool {
    if name.contains("::") || name.contains(' ') || name.contains('/') {
        return false;
    }
    let Some((service, method)) = name.split_once('.') else {
        return false;
    };
    if service.is_empty() || method.is_empty() || method.contains('.') {
        return false;
    }
    let Some(svc0) = service.chars().next() else {
        return false;
    };
    let Some(m0) = method.chars().next() else {
        return false;
    };
    // Service type PascalCase + method starting with letter/underscore
    svc0.is_ascii_uppercase()
        && (m0.is_ascii_alphabetic() || m0 == '_')
        && service
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && method
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Build an OTLP HTTP/protobuf tracer when `endpoint` is non-empty.
///
/// `endpoint` is the Vector OTLP/HTTP receiver base (`http://vector:4318`) or a
/// full traces URL ending in `/v1/traces`.
///
/// Note: `opentelemetry-otlp` 0.30 uses programmatic `with_endpoint` **as-is**
/// (it does **not** append `/v1/traces`), so the base URL is normalized here.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn build_otlp_tracer(options: OtlpExportOptions<'_>) -> Result<(Tracer, OtlpGuard)> {
    let endpoint = normalize_otlp_traces_endpoint(options.endpoint)?;
    anyhow::ensure!(!endpoint.is_empty(), "otel_endpoint is empty");

    let ratio = options.sample_ratio.clamp(0.0, 1.0);
    let root_sampler = if (ratio - 1.0).abs() < f64::EPSILON {
        Sampler::AlwaysOn
    } else if ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(ratio)
    };
    let sampler = Sampler::ParentBased(Box::new(root_sampler));

    let exporter = SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .build()
        .context("build OTLP HTTP span exporter")?;

    let resource = Resource::builder_empty()
        .with_attributes([KeyValue::new(
            SERVICE_NAME,
            options.service_name.to_string(),
        )])
        .build();

    let provider = SdkTracerProvider::builder()
        .with_span_processor(BoundedSpanProcessor::new(exporter, options.batch))
        .with_sampler(sampler)
        .with_id_generator(RandomIdGenerator::default())
        .with_span_limits(SpanLimits {
            max_attributes_per_span: MAX_ATTRIBUTES_PER_SPAN,
            max_events_per_span: MAX_EVENTS_PER_SPAN,
            max_links_per_span: MAX_LINKS_PER_SPAN,
            ..SpanLimits::default()
        })
        .with_resource(resource)
        .build();

    let tracer = provider.tracer(options.service_name.to_string());
    Ok((tracer, OtlpGuard { provider }))
}

/// Batch processor with an observable bound.
///
/// `pending` counts spans handed to the inner [`BatchSpanProcessor`] and not yet
/// taken by the exporter. At `max_queue_size` new spans are dropped here (and
/// counted) instead of inside the SDK queue, where drops are not observable.
/// String attribute values are capped at [`MAX_ATTRIBUTE_VALUE_BYTES`].
#[derive(Debug)]
struct BoundedSpanProcessor {
    inner: BatchSpanProcessor,
    pending: Arc<AtomicUsize>,
    capacity: usize,
}

impl BoundedSpanProcessor {
    fn new<E>(exporter: E, batch: OtlpBatchOptions) -> Self
    where
        E: opentelemetry_sdk::trace::SpanExporter + 'static,
    {
        let capacity = batch.max_queue_size.max(1);
        let pending = Arc::new(AtomicUsize::new(0));
        let config = BatchConfigBuilder::default()
            .with_max_queue_size(capacity)
            .with_max_export_batch_size(batch.max_export_batch_size.clamp(1, capacity))
            .with_scheduled_delay(batch.scheduled_delay)
            .build();
        let inner = BatchSpanProcessor::builder(CountingExporter {
            inner: exporter,
            pending: Arc::clone(&pending),
        })
        .with_batch_config(config)
        .build();
        Self {
            inner,
            pending,
            capacity,
        }
    }
}

impl SpanProcessor for BoundedSpanProcessor {
    fn on_start(&self, span: &mut opentelemetry_sdk::trace::Span, cx: &opentelemetry::Context) {
        self.inner.on_start(span, cx);
    }

    fn on_end(&self, mut span: SpanData) {
        if self.pending.fetch_add(1, Ordering::AcqRel) >= self.capacity {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            record_spans_dropped(SpanDropReason::QueueFull, 1);
            return;
        }
        truncate_span_values(&mut span);
        self.inner.on_end(span);
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.inner.set_resource(resource);
    }
}

/// Releases queue slots as batches leave the SDK queue; counts failed exports.
#[derive(Debug)]
struct CountingExporter<E> {
    inner: E,
    pending: Arc<AtomicUsize>,
}

impl<E> opentelemetry_sdk::trace::SpanExporter for CountingExporter<E>
where
    E: opentelemetry_sdk::trace::SpanExporter,
{
    async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
        let spans = batch.len();
        self.pending.fetch_sub(spans, Ordering::AcqRel);
        let result = self.inner.export(batch).await;
        if result.is_err() {
            record_spans_dropped(SpanDropReason::ExportFailed, spans);
        }
        result
    }

    fn shutdown_with_timeout(&mut self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn force_flush(&mut self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.inner.set_resource(resource);
    }
}

fn truncate_span_values(span: &mut SpanData) {
    span.attributes.iter_mut().for_each(truncate_attribute);
    for event in &mut span.events.events {
        event.attributes.iter_mut().for_each(truncate_attribute);
    }
}

fn truncate_attribute(attribute: &mut KeyValue) {
    match &mut attribute.value {
        Value::String(value) => {
            if let Some(cut) = truncated(value.as_str()) {
                *value = StringValue::from(cut);
            }
        }
        Value::Array(Array::String(values)) => {
            for value in values {
                if let Some(cut) = truncated(value.as_str()) {
                    *value = StringValue::from(cut);
                }
            }
        }
        _ => {}
    }
}

/// `Some(prefix)` when `value` exceeds [`MAX_ATTRIBUTE_VALUE_BYTES`].
fn truncated(value: &str) -> Option<String> {
    if value.len() <= MAX_ATTRIBUTE_VALUE_BYTES {
        return None;
    }
    let mut end = MAX_ATTRIBUTE_VALUE_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    Some(value[..end].to_string())
}

/// Ensure the exporter URL targets OTLP/HTTP traces.
///
/// - `…/v1/traces` → unchanged
/// - base (e.g. `http://vector:4318`) → `{base}/v1/traces`
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn normalize_otlp_traces_endpoint(endpoint: &str) -> Result<String> {
    let endpoint = endpoint.trim().trim_end_matches('/');
    anyhow::ensure!(!endpoint.is_empty(), "otel_endpoint is empty");
    if endpoint.ends_with("/v1/traces") {
        return Ok(endpoint.to_string());
    }
    Ok(format!("{endpoint}/v1/traces"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::OTLP_SPANS_DROPPED_TOTAL;

    #[test]
    fn allow_listed_names() {
        assert!(is_exported_span_name("request"));
        assert!(is_exported_span_name("balance_check"));
        assert!(is_exported_span_name("db.query"));
        assert!(is_exported_span_name("upstream.attempt"));
        assert!(is_exported_span_name("CacheService.get"));
        assert!(!is_exported_span_name("FramedRead::poll_next"));
        assert!(!is_exported_span_name("poll_ready"));
        assert!(!is_exported_span_name("try_reclaim_frame"));
        assert!(!is_exported_span_name("Connection"));
    }

    #[test]
    fn rpc_style_names() {
        assert!(is_rpc_style_span_name("CacheService.get"));
        assert!(is_rpc_style_span_name("AuthService.validate_api_key"));
        assert!(is_rpc_style_span_name("CBProService.store_response"));
        assert!(is_rpc_style_span_name(
            "RateLimitService.ingest_usage_deltas"
        ));
        assert!(!is_rpc_style_span_name("request"));
        assert!(!is_rpc_style_span_name("FramedRead::poll_next"));
        assert!(!is_rpc_style_span_name("foo.bar.baz"));
        assert!(!is_rpc_style_span_name("lowercase.method"));
    }

    #[test]
    fn filter_keeps_business_drops_h2() {
        use tracing::info_span;

        // Use a temporary default subscriber so span metadata is available.
        let _guard = tracing::subscriber::set_default(tracing_subscriber::registry());

        let req = info_span!(
            target: "mazbri_service::middleware",
            "request",
            request_id = "r1"
        );
        assert!(should_export_otlp_span(req.metadata().expect("meta")));

        let rpc = info_span!(target: "tarpc::client", "CacheService.get");
        assert!(should_export_otlp_span(rpc.metadata().expect("meta")));

        let tower_req = info_span!(target: "tower_http::trace::make_span", "request");
        assert!(should_export_otlp_span(tower_req.metadata().expect("meta")));

        let unknown = info_span!(target: "sqlx::query", "fetch");
        assert!(!should_export_otlp_span(unknown.metadata().expect("meta")));

        let debug_app = tracing::debug_span!(target: "mazbri_gateway::handler", "inner");
        assert!(!should_export_otlp_span(
            debug_app.metadata().expect("meta")
        ));

        let h2 = tracing::trace_span!(
            target: "h2::proto::streams::prioritize",
            "try_reclaim_frame"
        );
        assert!(!should_export_otlp_span(h2.metadata().expect("meta")));

        let framed = tracing::debug_span!(
            target: "h2::codec::framed_read",
            "FramedRead::poll_next"
        );
        assert!(!should_export_otlp_span(framed.metadata().expect("meta")));
    }

    #[test]
    fn long_string_values_cut_on_char_boundary() {
        let short = "a".repeat(MAX_ATTRIBUTE_VALUE_BYTES);
        assert!(truncated(&short).is_none());
        let long = format!("{}{}", "a".repeat(MAX_ATTRIBUTE_VALUE_BYTES - 1), "中文");
        let cut = truncated(&long).expect("cut");
        assert_eq!(cut.len(), MAX_ATTRIBUTE_VALUE_BYTES - 1);

        let mut attribute = KeyValue::new("k", "x".repeat(4096));
        truncate_attribute(&mut attribute);
        assert_eq!(attribute.value.as_str().len(), MAX_ATTRIBUTE_VALUE_BYTES);
    }

    #[derive(Debug)]
    struct NullExporter;

    impl opentelemetry_sdk::trace::SpanExporter for NullExporter {
        fn export(
            &self,
            _batch: Vec<SpanData>,
        ) -> impl std::future::Future<Output = OTelSdkResult> + Send {
            std::future::ready(Ok(()))
        }
    }

    fn sample_span() -> SpanData {
        SpanData {
            span_context: opentelemetry::trace::SpanContext::empty_context(),
            parent_span_id: opentelemetry::trace::SpanId::INVALID,
            span_kind: opentelemetry::trace::SpanKind::Internal,
            name: "test".into(),
            start_time: std::time::SystemTime::now(),
            end_time: std::time::SystemTime::now(),
            attributes: vec![KeyValue::new("big", "y".repeat(2048))],
            dropped_attributes_count: 0,
            events: opentelemetry_sdk::trace::SpanEvents::default(),
            links: opentelemetry_sdk::trace::SpanLinks::default(),
            status: opentelemetry::trace::Status::Unset,
            instrumentation_scope: opentelemetry::InstrumentationScope::default(),
        }
    }

    #[test]
    fn full_queue_drops_and_counts() {
        let counter = OTLP_SPANS_DROPPED_TOTAL
            .with_label_values(&["unknown", SpanDropReason::QueueFull.as_str()]);
        let before = counter.get();
        let processor = BoundedSpanProcessor::new(
            NullExporter,
            OtlpBatchOptions {
                max_queue_size: 2,
                max_export_batch_size: 2,
                scheduled_delay: Duration::from_secs(3600),
            },
        );
        for _ in 0..5 {
            processor.on_end(sample_span());
        }
        assert!(counter.get() - before >= 3.0);
        processor.force_flush().expect("flush");
        assert_eq!(processor.pending.load(Ordering::Acquire), 0);
        processor.shutdown().expect("shutdown");
    }

    #[test]
    fn normalize_base_appends_traces_path() {
        assert_eq!(
            normalize_otlp_traces_endpoint("http://vector:4318").unwrap(),
            "http://vector:4318/v1/traces"
        );
        assert_eq!(
            normalize_otlp_traces_endpoint("http://vector:4318/").unwrap(),
            "http://vector:4318/v1/traces"
        );
        assert_eq!(
            normalize_otlp_traces_endpoint("http://vector:4318/v1/traces").unwrap(),
            "http://vector:4318/v1/traces"
        );
        assert!(normalize_otlp_traces_endpoint("  ").is_err());
    }

    /// Live smoke against the Vector OTLP receiver (requires Tokio, same as production services).
    /// `MAZBRI_OTLP_SMOKE=1 MAZBRI_OTLP_ENDPOINT=http://<vector>:4318 cargo test -p mazbri-core --lib otlp_export_smoke -- --ignored --nocapture`
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "live OTLP receiver; set MAZBRI_OTLP_SMOKE=1"]
    async fn otlp_export_smoke() {
        use opentelemetry::KeyValue;
        use opentelemetry::trace::{Span, Tracer as _};
        if std::env::var("MAZBRI_OTLP_SMOKE").ok().as_deref() != Some("1") {
            eprintln!("skip: set MAZBRI_OTLP_SMOKE=1");
            return;
        }
        let endpoint =
            std::env::var("MAZBRI_OTLP_ENDPOINT").unwrap_or_else(|_| "http://vector:4318".into());
        let (tracer, guard) = build_otlp_tracer(OtlpExportOptions {
            endpoint: &endpoint,
            service_name: "otlp-smoke",
            sample_ratio: 1.0,
            batch: OtlpBatchOptions {
                max_queue_size: 64,
                max_export_batch_size: 64,
                scheduled_delay: Duration::from_millis(200),
            },
        })
        .expect("build tracer");
        let mut span = tracer.start("otlp.smoke");
        span.set_attribute(KeyValue::new("mazbri.smoke", "true"));
        span.end();
        drop(guard);
        // Batch exporter flush on shutdown; brief pause for HTTP delivery.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
