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

use anyhow::{Context, Result};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, SdkTracerProvider, Tracer};
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;
use tracing::Level;

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
}

/// Whether a `tracing` span should be exported via OTLP.
///
/// **Keep (useful for call path / latency / dependency hops):**
/// - HTTP root: name `request` (mazbri middleware / tower-http `TraceLayer`)
/// - tarpc client/server RPC: names like `CacheService.get`, `AuthService.validate_api_key`
/// - application targets: `mazbri*`, `ut*` modules
///
/// **Drop (no help for perf / SLI / path analysis):**
/// - `h2` / hyper / mio / rustls transport internals (`FramedRead::*`, `poll_ready`, …)
/// - DEBUG / TRACE level spans
/// - other dependency noise not matching the allow rules
#[must_use]
pub fn should_export_otlp_span(meta: &tracing::Metadata<'_>) -> bool {
    // Level ordering: ERROR < WARN < INFO < DEBUG < TRACE
    if *meta.level() > Level::INFO {
        return false;
    }

    let target = meta.target();
    let name = meta.name();

    if is_transport_target(target) || is_transport_span_name(name) {
        return false;
    }

    if is_application_target(target)
        || is_http_entry_target(target)
        || is_tarpc_target(target)
        || name == "request"
        || is_rpc_style_span_name(name)
    {
        return true;
    }

    false
}

#[inline]
fn is_application_target(target: &str) -> bool {
    target.starts_with("mazbri")
}

#[inline]
fn is_http_entry_target(target: &str) -> bool {
    // tower-http TraceLayer (HTTP entry when service uses TraceLayer)
    target.starts_with("tower_http::trace")
}

#[inline]
fn is_tarpc_target(target: &str) -> bool {
    // tarpc instruments RPC methods as `ServiceName.method` under tarpc::client / server
    target.starts_with("tarpc::client") || target.starts_with("tarpc::server") || target == "tarpc"
}

/// h2 / hyper / low-level IO — zero value for product path analysis.
fn is_transport_target(target: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "h2::",
        "h2.",
        "hyper::",
        "hyper.",
        "mio::",
        "want::",
        "rustls::",
        "tokio::io",
        "tokio::net",
        "tokio::task",
        "tower::buffer",
        "tower::limit",
        "tower::make",
        "tower::util",
        "http_body",
        "httparse",
    ];
    target == "h2" || target == "hyper" || PREFIXES.iter().any(|p| target.starts_with(p))
}

/// Span names observed flooding Greptime from h2 (even if target is missing/odd).
fn is_transport_span_name(name: &str) -> bool {
    // `FramedRead::poll_next`, `hpack::decode`
    if name.contains("::") {
        return true;
    }
    matches!(
        name,
        "poll"
            | "poll_ready"
            | "popped"
            | "flush"
            | "Connection"
            | "server_handshake"
            | "read_preface"
            | "send_data"
            | "reserve_capacity"
            | "assign_connection_capacity"
            | "clear_pending_capacity"
            | "try_reclaim_frame"
            | "try_assign_capacity"
            | "pop_frame"
            | "recv_stream_window_update"
    ) || name.starts_with("Framed")
        || name.starts_with("hpack")
        || name.starts_with("Prioritize")
        || name.starts_with("updating ")
        || name.starts_with("try_reclaim")
        || name.starts_with("try_assign")
        || name.starts_with("poll_")
        || name.starts_with("pop_")
        || name.starts_with("send_")
        || name.starts_with("recv_")
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
    let sampler = if (ratio - 1.0).abs() < f64::EPSILON {
        Sampler::AlwaysOn
    } else if ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(ratio)
    };

    let exporter = SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .build()
        .context("build OTLP HTTP span exporter")?;

    let resource = Resource::builder_empty()
        .with_attributes([opentelemetry::KeyValue::new(
            SERVICE_NAME,
            options.service_name.to_string(),
        )])
        .build();

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_sampler(sampler)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(resource)
        .build();

    let tracer = provider.tracer(options.service_name.to_string());
    Ok((tracer, OtlpGuard { provider }))
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

    #[test]
    fn transport_targets_and_names_detected() {
        assert!(is_transport_target("h2::proto::streams::prioritize"));
        assert!(is_transport_target("hyper::client::conn"));
        assert!(is_transport_span_name("FramedRead::poll_next"));
        assert!(is_transport_span_name("poll_ready"));
        assert!(is_transport_span_name("try_reclaim_frame"));
        assert!(is_transport_span_name("updating stream flow"));
        assert!(!is_transport_span_name("request"));
        assert!(!is_transport_span_name("CacheService.get"));
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
