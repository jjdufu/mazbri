use crate::trace_layer::is_probe_unavailable;
use axum::http::HeaderValue;
use axum::{extract::Request, middleware::Next, response::Response};
use mazbri_core::trace_propagation::{remote_parent_from_headers, set_remote_parent};
use mazbri_core::{
    ACTIVE_CONNECTIONS, HTTP_REQUEST_DURATION_SECONDS, HTTP_REQUESTS_TOTAL, TraceContext,
    request_id_from_headers,
};
use std::sync::Arc;
use std::time::Instant;
use tracing::{Instrument, debug, error, info_span, warn};

/// 是否为高频探针或节点心跳路径：保留本地 `INFO` 日志诊断上下文，但由 OTLP 导出层前置拦截。
fn is_probe_or_heartbeat_path(path: &str) -> bool {
    matches!(
        path,
        "/health" | "/ready" | "/metrics" | "/nodes/heartbeat"
    )
}

/// HTTP 入口：以入站 `traceparent` 为父节点创建 `request`（或探针专用 `probe_request`）span，
/// 并把本段链路 ID 写回请求头（供下游处理函数读取）与响应头（`traceparent`）。
pub async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    let request_id = request_id_from_headers(req.headers());
    let remote_parent = remote_parent_from_headers(req.headers());
    let parent_span_id: Arc<str> = remote_parent.as_ref().map_or_else(
        || Arc::from(""),
        |parent| parent.span_id().to_string().into(),
    );
    let path = req.uri().path();
    let span = if is_probe_or_heartbeat_path(path) {
        info_span!(
            "probe_request",
            otel.kind = "server",
            request_id = %request_id,
            trace_id = tracing::field::Empty,
            span_id = tracing::field::Empty,
            parent_span_id = %parent_span_id,
            method = %req.method(),
            path = %path
        )
    } else {
        info_span!(
            "request",
            otel.kind = "server",
            request_id = %request_id,
            trace_id = tracing::field::Empty,
            span_id = tracing::field::Empty,
            parent_span_id = %parent_span_id,
            method = %req.method(),
            path = %path
        )
    };
    if let Some(parent) = remote_parent {
        set_remote_parent(&span, parent);
    }
    let trace_context = TraceContext::for_span(request_id, &span, parent_span_id);
    span.record("trace_id", trace_context.trace_id.as_ref());
    span.record("span_id", trace_context.span_id.as_ref());
    insert_trace_headers(req.headers_mut(), &trace_context);
    req.extensions_mut().insert(trace_context.clone());
    // Must use Instrument (not span.enter()) across .await — enter() across yield
    // races tracing-subscriber's sharded registry and panics under concurrency.
    let mut response = async move {
        debug!("Handling request");
        next.run(req).await
    }
    .instrument(span)
    .await;
    set_header(
        response.headers_mut(),
        "traceparent",
        &trace_context.traceparent_value(),
    );
    response
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
    if is_probe_unavailable(uri.path(), response.status()) {
        warn!(uri = % uri, status = % response.status(), "Health probe reported unavailable");
    } else if response.status().is_server_error() {
        error!(uri = % uri, status = % response.status(), "Server error occurred");
    } else if response.status().is_client_error() {
        debug!(uri = % uri, status = % response.status(), "Client error occurred");
    }
    response
}

#[cfg(test)]
mod probe_log_tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn probe_503_is_not_server_error_log() {
        assert!(is_probe_unavailable(
            "/health",
            StatusCode::SERVICE_UNAVAILABLE
        ));
        assert!(is_probe_unavailable(
            "/ready",
            StatusCode::SERVICE_UNAVAILABLE
        ));
        assert!(!is_probe_unavailable(
            "/health",
            StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(!is_probe_unavailable(
            "/v1/x",
            StatusCode::SERVICE_UNAVAILABLE
        ));
    }

    #[test]
    fn probe_or_heartbeat_paths_match_only_noisy_endpoints() {
        assert!(is_probe_or_heartbeat_path("/health"));
        assert!(is_probe_or_heartbeat_path("/ready"));
        assert!(is_probe_or_heartbeat_path("/metrics"));
        assert!(is_probe_or_heartbeat_path("/nodes/heartbeat"));
        assert!(!is_probe_or_heartbeat_path("/nodes/register"));
        assert!(!is_probe_or_heartbeat_path("/nodes"));
        assert!(!is_probe_or_heartbeat_path("/v1/query"));
    }
}
