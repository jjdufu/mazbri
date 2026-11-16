use crate::trace_layer::is_probe_unavailable;
use axum::http::HeaderValue;
use axum::{extract::Request, middleware::Next, response::Response};
use mazbri_core::trace_propagation::{remote_parent_from_headers, set_remote_parent};
use mazbri_core::{
    ACTIVE_CONNECTIONS, HTTP_REQUEST_DURATION_SECONDS, HTTP_REQUESTS_TOTAL, TraceContext,
    inbound_request_id, scope_request_trace,
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
/// 并把本段链路 ID 写回请求头（供下游处理函数读取）与响应头（`x-request-id`、`traceparent`）。
///
/// 入站 `X-Request-Id` 合规（`[A-Za-z0-9._-]{1,128}`）原样沿用，否则换成新 UUID、原值记入
/// `client_request_id`。处理函数在 [`scope_request_trace`] 作用域内运行，错误响应体可取到真实请求 ID。
pub async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    let inbound = inbound_request_id(req.headers());
    let request_id = inbound.request_id;
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
    let trace_context = TraceContext::for_span(request_id, &span, parent_span_id)
        .with_client_request_id(inbound.client_request_id);
    if !trace_context.client_request_id.is_empty() {
        debug!(
            request_id = %trace_context.request_id,
            client_request_id = %trace_context.client_request_id,
            "Replaced non-conforming inbound x-request-id"
        );
    }
    span.record("trace_id", trace_context.trace_id.as_ref());
    span.record("span_id", trace_context.span_id.as_ref());
    insert_trace_headers(req.headers_mut(), &trace_context);
    req.extensions_mut().insert(trace_context.clone());
    // Must use Instrument (not span.enter()) across .await — enter() across yield
    // races tracing-subscriber's sharded registry and panics under concurrency.
    let mut response = scope_request_trace(
        trace_context.clone(),
        async move {
            debug!("Handling request");
            next.run(req).await
        }
        .instrument(span),
    )
    .await;
    insert_response_trace_headers(response.headers_mut(), &trace_context);
    response
}

/// 每个响应（含错误响应）都回写 `x-request-id` 与 `traceparent`。
fn insert_response_trace_headers(
    headers: &mut axum::http::HeaderMap,
    trace_context: &TraceContext,
) {
    set_header(headers, "x-request-id", &trace_context.request_id);
    set_header(headers, "traceparent", &trace_context.traceparent_value());
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

#[cfg(test)]
mod request_id_middleware_tests {
    use super::request_id_middleware;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use tower::ServiceExt;

    fn app() -> Router {
        Router::new()
            .route(
                "/ok",
                get(|| async {
                    mazbri_core::current_request_trace()
                        .map(|trace| format!("{}|{}", trace.request_id, trace.client_request_id))
                        .unwrap_or_default()
                }),
            )
            .route("/fail", get(|| async { StatusCode::UNAUTHORIZED }))
            .layer(axum::middleware::from_fn(request_id_middleware))
    }

    async fn call(
        path: &str,
        request_id: Option<&str>,
        traceparent: Option<&str>,
    ) -> axum::response::Response {
        let mut builder = Request::builder().uri(path);
        if let Some(id) = request_id {
            builder = builder.header("x-request-id", id);
        }
        if let Some(tp) = traceparent {
            builder = builder.header("traceparent", tp);
        }
        app()
            .oneshot(builder.body(Body::empty()).expect("request"))
            .await
            .expect("response")
    }

    fn header<'a>(response: &'a axum::response::Response, name: &str) -> &'a str {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
    }

    async fn body_text(response: axum::response::Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .expect("body");
        String::from_utf8(bytes.to_vec()).expect("utf8")
    }

    #[tokio::test]
    async fn echoes_conforming_request_id_on_success_and_error() {
        let response = call("/ok", Some("client-abc_1.2"), None).await;
        assert_eq!(header(&response, "x-request-id"), "client-abc_1.2");
        assert!(header(&response, "traceparent").starts_with("00-"));
        assert_eq!(body_text(response).await, "client-abc_1.2|");

        let response = call("/fail", Some("client-abc_1.2"), None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(header(&response, "x-request-id"), "client-abc_1.2");
    }

    #[tokio::test]
    async fn replaces_non_conforming_request_id_and_keeps_original() {
        let response = call("/ok", Some("bad id/1"), None).await;
        let echoed = header(&response, "x-request-id").to_string();
        assert!(uuid::Uuid::parse_str(&echoed).is_ok());
        assert_eq!(body_text(response).await, format!("{echoed}|bad id/1"));
    }

    #[tokio::test]
    async fn traceparent_flags_follow_inbound_without_otel_layer() {
        // 无 OpenTelemetry 层时不导出任何 span，sampled 为 00，绝不写死 01。
        let response = call(
            "/ok",
            None,
            Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
        )
        .await;
        assert!(header(&response, "traceparent").ends_with("-00"));
    }
}
