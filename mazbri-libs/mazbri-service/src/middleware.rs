use axum::http::HeaderValue;
use axum::{extract::Request, middleware::Next, response::Response};
use mazbri_core::{
    ACTIVE_CONNECTIONS, HTTP_REQUEST_DURATION_SECONDS, HTTP_REQUESTS_TOTAL, TraceContext,
    parse_trace_context,
};
use std::time::Instant;
use tracing::{Instrument, debug, error, info_span};
pub async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    let trace_context = parse_trace_context(req.headers());
    insert_trace_headers(req.headers_mut(), &trace_context);
    req.extensions_mut().insert(trace_context.clone());
    let span = info_span!(
        "request",
        request_id = %trace_context.request_id,
        trace_id = %trace_context.trace_id,
        span_id = %trace_context.span_id,
        parent_span_id = %trace_context.parent_span_id,
        method = %req.method(),
        path = %req.uri().path()
    );
    // Must use Instrument (not span.enter()) across .await — enter() across yield
    // races tracing-subscriber's sharded registry and panics under concurrency.
    async move {
        debug!("Handling request");
        next.run(req).await
    }
    .instrument(span)
    .await
}

fn insert_trace_headers(headers: &mut axum::http::HeaderMap, trace_context: &TraceContext) {
    set_header(headers, "x-request-id", &trace_context.request_id);
    set_header(headers, "x-trace-id", &trace_context.trace_id);
    set_header(headers, "x-span-id", &trace_context.span_id);
    set_header(headers, "x-parent-span-id", &trace_context.parent_span_id);
    set_header(headers, "traceparent", &trace_context.traceparent_value());
}

fn set_header(headers: &mut axum::http::HeaderMap, name: &'static str, value: &str) {
    match value.parse::<HeaderValue>() {
        Ok(header_value) => {
            headers.insert(name, header_value);
        }
        Err(_) => {
            debug!(header = name, "无法设置 trace header，跳过");
        }
    }
}
fn normalize_metric_path(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if segment.len() == 36 && uuid::Uuid::parse_str(segment).is_ok() {
                ":uuid"
            } else if segment.chars().all(|c| c.is_ascii_digit()) && !segment.is_empty() {
                ":id"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}
pub async fn metrics_middleware(req: Request, next: Next) -> Response {
    struct ActiveRequestGuard;
    impl ActiveRequestGuard {
        fn new() -> Self {
            ACTIVE_CONNECTIONS.inc();
            Self
        }
    }
    impl Drop for ActiveRequestGuard {
        fn drop(&mut self) {
            ACTIVE_CONNECTIONS.dec();
        }
    }
    let _active_request = ActiveRequestGuard::new();
    let method = req.method().to_string();
    let path = normalize_metric_path(req.uri().path());
    let start = Instant::now();
    let response = next.run(req).await;
    let status = response.status().as_u16().to_string();
    let duration = start.elapsed();
    HTTP_REQUESTS_TOTAL
        .with_label_values(&[&method, &path, &status])
        .inc();
    HTTP_REQUEST_DURATION_SECONDS
        .with_label_values(&[&method, &path])
        .observe(duration.as_secs_f64());
    response
}
pub async fn error_handler_middleware(req: Request, next: Next) -> Response {
    let uri = req.uri().clone();
    let response = next.run(req).await;
    if response.status().is_server_error() {
        error!(uri = % uri, status = % response.status(), "Server error occurred");
    } else if response.status().is_client_error() {
        debug!(uri = % uri, status = % response.status(), "Client error occurred");
    }
    response
}
