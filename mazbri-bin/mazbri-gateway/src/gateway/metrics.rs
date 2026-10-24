use axum::http::HeaderMap;
use axum::response::IntoResponse;
use mazbri_core::metrics::{MetricsBuilder, REGISTRY, register_collector_once};
use prometheus::{CounterVec, Encoder, HistogramVec, IntCounter, IntGauge, Opts, TextEncoder};
use std::sync::LazyLock;

/// mode: transparent | microservice | unknown
/// result: ok | `client_error` | `upstream_error` | timeout | overload | authz | ratelimit | config | `circuit_open` | internal
pub static API_PROXY_REQUESTS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "api_proxy_requests_total",
        "Total number of API proxy requests",
        &["product_id", "method", "status", "mode", "result"],
    )
});
pub static API_PROXY_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "api_proxy_duration_seconds",
        "API proxy request duration in seconds",
        vec![
            0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ],
        &["product_id", "method", "mode"],
    )
});
pub static ACTIVE_PROXY_CONNECTIONS: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge(
        "active_proxy_connections",
        "Number of active proxy connections",
    )
});
/// 网关本地段（duration - `upstream_wait），不含真实上游业务时间中的出站等待`。
pub static GATEWAY_PLATFORM_LOCAL_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "gateway_platform_local_seconds",
        "Gateway local processing time excluding upstream wait (auth/balance/serialize etc.)",
        vec![0.001, 0.002, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5],
        &["product_id"],
    )
});
/// 网关调用上游通道等待（含 bypass/微服务 + 其背后上游），用于拆分 platform hop 观察。
pub static GATEWAY_UPSTREAM_WAIT_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "gateway_upstream_wait_seconds",
        "Time spent waiting on upstream channel from gateway (includes bypass hop + remote)",
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5],
        &["product_id", "mode"],
    )
});
/// 上游尝试次数（含 switch 重试）。
pub static GATEWAY_UPSTREAM_ATTEMPTS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "gateway_upstream_attempts_total",
        "Upstream channel attempts by mode and result",
        &["mode", "result"],
    )
});
/// 未知/非法契约 ID。
pub static GATEWAY_UPSTREAM_CONTRACT_ERRORS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "gateway_upstream_contract_errors_total",
        "Upstream gRPC contract resolution failures",
        &["contract_id", "reason"],
    )
});
pub static BALANCE_PRECHECK_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "gateway_balance_precheck_total",
        "Balance precheck results by cache source and decision",
        &["source", "result"],
    )
});
/// Client TCP disconnect before HTTP message completed (phase × reason).
pub static GATEWAY_HTTP_CLIENT_DISCONNECT_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "gateway_http_client_disconnect_total",
        "Client disconnected before full HTTP response was delivered",
        &["phase", "reason"],
    )
});
/// Panic recovered by `CatchPanic` middleware.
pub static GATEWAY_PANIC_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::with_opts(Opts::new(
        "gateway_panic_total",
        "Number of panics caught by gateway CatchPanic layer",
    ))
    .expect("gateway_panic_total metric")
});
/// Overload rejections (`max_connections` concurrency limit).
pub static GATEWAY_REQUEST_BINDING_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "gateway_request_binding_total",
        "Tenant request binding results (ok, skipped, or machine code)",
        &["result"],
    )
});
pub static GATEWAY_JWT_KID_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "gateway_jwt_kid",
        "JWT verification by kid and result",
        &["kid", "result"],
    )
});
pub static GATEWAY_IDEMPOTENCY_UNAVAILABLE_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::with_opts(Opts::new(
        "gateway_idempotency_unavailable_total",
        "Keyed requests rejected with 503 because the idempotency store (Redis) failed",
    ))
    .expect("gateway_idempotency_unavailable_total metric")
});

pub static GATEWAY_OVERLOAD_REJECT_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::with_opts(Opts::new(
        "gateway_overload_reject_total",
        "Requests rejected with 503 due to max_connections",
    ))
    .expect("gateway_overload_reject_total metric")
});

/// Map HTTP status (+ optional error hint) to low-cardinality result label.
#[must_use]
pub fn classify_proxy_result(status: u16, error_hint: Option<&str>) -> &'static str {
    if let Some(hint) = error_hint {
        let h = hint.to_ascii_lowercase();
        if h.contains("timeout") {
            return "timeout";
        }
        if h.contains("rate") || h.contains("ratelimit") {
            return "ratelimit";
        }
        if h.contains("circuit") {
            return "circuit_open";
        }
        if h.contains("contract") || h.contains("config") {
            return "config";
        }
        if h.contains("overload") || h.contains("max_connection") {
            return "overload";
        }
    }
    match status {
        200..=299 => "ok",
        401 | 403 => "authz",
        429 => "ratelimit",
        400..=499 => "client_error",
        504 => "timeout",
        503 => "overload",
        502 => "upstream_error",
        _ => "internal",
    }
}

/// Map status/error to `error_class` for access logs (aligned with metrics result).
#[must_use]
pub fn classify_error_class(status: u16, is_success: bool, error_code: &str) -> String {
    if is_success {
        return "ok".to_string();
    }
    classify_proxy_result(status, Some(error_code)).to_string()
}

/// gateway→upstream (bypass) HTTP/2 pool: gRPC calls currently holding a stream.
pub static GATEWAY_UPSTREAM_H2_STREAMS_IN_USE: LazyLock<prometheus::IntGaugeVec> =
    LazyLock::new(|| {
        MetricsBuilder::new().gauge_vec(
            "gateway_upstream_h2_streams_in_use",
            "In-flight gRPC calls (HTTP/2 streams) per upstream endpoint",
            &["endpoint"],
        )
    });
/// Time spent in `Grpc::ready()` (waiting for channel/connection capacity) before a call.
pub static GATEWAY_UPSTREAM_H2_READY_WAIT_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "gateway_upstream_h2_ready_wait_seconds",
        "Wait for the HTTP/2 channel to accept a new stream (connection readiness)",
        vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0],
        &["endpoint"],
    )
});
/// Explicit connection establishment (TCP + mTLS + h2 handshake) duration.
pub static GATEWAY_UPSTREAM_H2_CONNECT_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "gateway_upstream_h2_connect_seconds",
        "Upstream HTTP/2 connect (TCP+mTLS+h2) duration",
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0],
        &["endpoint", "result"],
    )
});
/// Connection (re)establishments: reason = initial | refresh, result = ok | error.
pub static GATEWAY_UPSTREAM_H2_CONNECTS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "gateway_upstream_h2_connects_total",
        "Upstream HTTP/2 connection (re)establishments",
        &["endpoint", "reason", "result"],
    )
});
/// Call/readiness errors by kind (goaway | reset | `broken_pipe` | refused | timeout | other).
/// Transparent tonic reconnects that never surface an error are not visible here.
pub static GATEWAY_UPSTREAM_H2_ERRORS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "gateway_upstream_h2_errors_total",
        "Upstream HTTP/2 call errors by classified kind",
        &["endpoint", "stage", "kind"],
    )
});

/// Classify a transport error message into a low-cardinality kind.
#[must_use]
pub fn classify_h2_error(message: &str) -> &'static str {
    let m = message.to_ascii_lowercase();
    if m.contains("goaway") || m.contains("go_away") || m.contains("go away") {
        "goaway"
    } else if m.contains("reset") || m.contains("rst_stream") {
        "reset"
    } else if m.contains("broken pipe") {
        "broken_pipe"
    } else if m.contains("refused") {
        "refused"
    } else if m.contains("timeout") || m.contains("timed out") || m.contains("deadline") {
        "timeout"
    } else {
        "other"
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_proxy_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(API_PROXY_REQUESTS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(API_PROXY_DURATION_SECONDS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(ACTIVE_PROXY_CONNECTIONS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_PLATFORM_LOCAL_SECONDS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_UPSTREAM_WAIT_SECONDS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_UPSTREAM_ATTEMPTS_TOTAL.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(GATEWAY_UPSTREAM_CONTRACT_ERRORS_TOTAL.clone()),
    )?;
    register_collector_once(&REGISTRY, Box::new(BALANCE_PRECHECK_TOTAL.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(GATEWAY_HTTP_CLIENT_DISCONNECT_TOTAL.clone()),
    )?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_PANIC_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_OVERLOAD_REJECT_TOTAL.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(GATEWAY_IDEMPOTENCY_UNAVAILABLE_TOTAL.clone()),
    )?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_REQUEST_BINDING_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_JWT_KID_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_UPSTREAM_H2_STREAMS_IN_USE.clone()))?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_UPSTREAM_H2_READY_WAIT_SECONDS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_UPSTREAM_H2_CONNECT_SECONDS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_UPSTREAM_H2_CONNECTS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_UPSTREAM_H2_ERRORS_TOTAL.clone()))?;
    Ok(())
}

/// Record a client disconnect and emit stage log (reuse gateway.stage, no new `log_route`).
pub fn record_client_disconnect(
    phase: &str,
    reason: &str,
    peer: Option<&str>,
    request_id: Option<&str>,
    error: Option<&str>,
) {
    GATEWAY_HTTP_CLIENT_DISCONNECT_TOTAL
        .with_label_values(&[phase, reason])
        .inc();
    tracing::warn!(
        target: "gateway.stage",
        event = "client_disconnect",
        phase = phase,
        reason = reason,
        peer = peer.unwrap_or(""),
        request_id = request_id.unwrap_or(""),
        error = error.unwrap_or(""),
        message = "Client disconnected before HTTP message completed"
    );
}
pub(crate) struct ConnectionGuard;
impl ConnectionGuard {
    pub(crate) fn new() -> Self {
        ACTIVE_PROXY_CONNECTIONS.inc();
        Self
    }
}
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        ACTIVE_PROXY_CONNECTIONS.dec();
    }
}
pub(crate) fn extract_user_agent(headers: &HeaderMap) -> String {
    headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string()
}
pub async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = Vec::new();
    match encoder.encode(&metric_families, &mut buffer) {
        Ok(()) => {
            String::from_utf8(buffer).unwrap_or_else(|_| "# Metrics encoding error\n".to_string())
        }
        Err(e) => {
            tracing::error!("Failed to encode metrics: {}", e);
            format!("# ERROR: Failed to encode metrics: {e}\n")
        }
    }
}

#[cfg(test)]
mod h2_metric_tests {
    use super::classify_h2_error;

    #[test]
    fn classifies_transport_errors() {
        assert_eq!(classify_h2_error("connection error: GoAway(..)"), "goaway");
        assert_eq!(classify_h2_error("stream error: RST_STREAM"), "reset");
        assert_eq!(classify_h2_error("Broken pipe (os error 32)"), "broken_pipe");
        assert_eq!(classify_h2_error("Connection refused"), "refused");
        assert_eq!(classify_h2_error("deadline has elapsed"), "timeout");
        assert_eq!(classify_h2_error("weird"), "other");
    }
}
