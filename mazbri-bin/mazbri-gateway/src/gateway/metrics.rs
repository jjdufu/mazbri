use axum::http::HeaderMap;
use axum::response::IntoResponse;
use mazbri_core::metrics::{MetricsBuilder, REGISTRY, register_collector_once};
use prometheus::{CounterVec, Encoder, HistogramVec, IntCounter, IntGauge, Opts, TextEncoder};
use std::sync::LazyLock;

/// mode: transparent | microservice | unknown
/// result: ok | client_error | upstream_error | timeout | overload | authz | ratelimit | config | circuit_open | internal
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
/// 网关本地段（duration - upstream_wait），不含真实上游业务时间中的出站等待。
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
/// Panic recovered by CatchPanic middleware.
pub static GATEWAY_PANIC_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::with_opts(Opts::new(
        "gateway_panic_total",
        "Number of panics caught by gateway CatchPanic layer",
    ))
    .expect("gateway_panic_total metric")
});
/// Overload rejections (max_connections concurrency limit).
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
pub static GATEWAY_OVERLOAD_REJECT_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::with_opts(Opts::new(
        "gateway_overload_reject_total",
        "Requests rejected with 503 due to max_connections",
    ))
    .expect("gateway_overload_reject_total metric")
});

/// Map HTTP status (+ optional error hint) to low-cardinality result label.
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
        500..=599 => "internal",
        _ => "internal",
    }
}

/// Map status/error to error_class for access logs (aligned with metrics result).
pub fn classify_error_class(status: u16, is_success: bool, error_code: &str) -> String {
    if is_success {
        return "ok".to_string();
    }
    classify_proxy_result(status, Some(error_code)).to_string()
}

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
    register_collector_once(&REGISTRY, Box::new(GATEWAY_REQUEST_BINDING_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_JWT_KID_TOTAL.clone()))?;
    Ok(())
}

/// Record a client disconnect and emit stage log (reuse gateway.stage, no new log_route).
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
        Ok(_) => {
            String::from_utf8(buffer).unwrap_or_else(|_| "# Metrics encoding error\n".to_string())
        }
        Err(e) => {
            tracing::error!("Failed to encode metrics: {}", e);
            format!("# ERROR: Failed to encode metrics: {}\n", e)
        }
    }
}
