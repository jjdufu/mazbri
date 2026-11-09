use mazbri_core::metrics::MetricsBuilder;
use mazbri_core::metrics::REGISTRY;
use mazbri_core::metrics::register_collector_once;
use mazbri_upstream_runtime::observe::{RuntimeEvent, RuntimeObserver};
use prometheus::{CounterVec, HistogramVec};
use std::sync::LazyLock;

/// `status_class`: 2xx|3xx|4xx|5xx|0
/// result: `ok|timeout|transport|ssrf|auth_token|config|transform`
pub static BYPASS_UPSTREAM_REQUESTS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_upstream_requests_total",
        "Total number of upstream requests executed by the mazbri-bypass",
        &["method", "status_class", "result"],
    )
});

pub static BYPASS_UPSTREAM_ERRORS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_upstream_errors_total",
        "Total number of upstream failures executed by the mazbri-bypass",
        &["method", "kind"],
    )
});

pub static BYPASS_UPSTREAM_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "bypass_upstream_duration_seconds",
        "Duration of mazbri-bypass upstream requests in seconds",
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
        &["method", "result"],
    )
});

pub static BYPASS_SSRF_BLOCK_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_ssrf_block_total",
        "SSRF blocks by reason",
        &["reason"],
    )
});

/// result: `ok|error|timeout|cache_hit|cache_miss`
pub static BYPASS_AUTH_TOKEN_FETCH_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_auth_token_fetch_total",
        "Auth token fetch outcomes",
        &["result"],
    )
});

/// result: hit|miss|invalid
pub static BYPASS_CONFIG_CACHE_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_config_cache_total",
        "Supplier config cache outcomes on bypass",
        &["result"],
    )
});

/// 判定为认证被拒的次数（每轮调用记一次）。`trigger`: `http_status|jsonrpc_code|body_match|ws_close`
pub static BYPASS_UPSTREAM_AUTH_REJECTED_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_upstream_auth_rejected_total",
        "Upstream responses classified as credential rejection",
        &["adapter", "trigger"],
    )
});

/// result: `ok|rejected_again|failed|skipped_disabled|skipped_budget`
pub static BYPASS_UPSTREAM_AUTH_RETRY_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_upstream_auth_retry_total",
        "Outcomes of token-refresh retries after credential rejection",
        &["adapter", "result"],
    )
});

/// result: `evicted|already_rotated`
pub static BYPASS_AUTH_TOKEN_EVICT_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_auth_token_evict_total",
        "Compare-and-evict results for rejected cached tokens",
        &["result"],
    )
});

/// stage: `checkout|send`
pub static BYPASS_WS_POOL_STALE_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_ws_pool_stale_total",
        "Dead pooled ws connections detected on checkout or before reply",
        &["stage"],
    )
});

/// result: `ok|failed|skipped_disabled|skipped_budget`
pub static BYPASS_WS_STALE_RETRY_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_ws_stale_retry_total",
        "Outcomes of fresh-connection retries after a reused ws connection closed",
        &["result"],
    )
});

/// kind: `auth|ws_stale`
pub static BYPASS_RETRY_BUDGET_EXHAUSTED_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "bypass_retry_budget_exhausted_total",
        "Retries denied by the retry budget",
        &["kind"],
    )
});

pub fn register_upstream_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(BYPASS_UPSTREAM_REQUESTS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BYPASS_UPSTREAM_ERRORS_TOTAL.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(BYPASS_UPSTREAM_DURATION_SECONDS.clone()),
    )?;
    register_collector_once(&REGISTRY, Box::new(BYPASS_SSRF_BLOCK_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BYPASS_AUTH_TOKEN_FETCH_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(BYPASS_CONFIG_CACHE_TOTAL.clone()))?;
    for counter in [
        &*BYPASS_UPSTREAM_AUTH_REJECTED_TOTAL,
        &*BYPASS_UPSTREAM_AUTH_RETRY_TOTAL,
        &*BYPASS_AUTH_TOKEN_EVICT_TOTAL,
        &*BYPASS_WS_POOL_STALE_TOTAL,
        &*BYPASS_WS_STALE_RETRY_TOTAL,
        &*BYPASS_RETRY_BUDGET_EXHAUSTED_TOTAL,
    ] {
        register_collector_once(&REGISTRY, Box::new(counter.clone()))?;
    }
    Ok(())
}

/// 运行时内部事件 → bypass 指标（标签均为闭集）。
pub struct BypassRuntimeObserver;

impl RuntimeObserver for BypassRuntimeObserver {
    fn on_event(&self, event: RuntimeEvent) {
        match event {
            RuntimeEvent::AuthRejected { adapter, trigger } => BYPASS_UPSTREAM_AUTH_REJECTED_TOTAL
                .with_label_values(&[adapter.as_str(), trigger.label()])
                .inc(),
            RuntimeEvent::AuthRetry { adapter, result } => BYPASS_UPSTREAM_AUTH_RETRY_TOTAL
                .with_label_values(&[adapter.as_str(), result.label()])
                .inc(),
            RuntimeEvent::TokenEvict { evicted } => BYPASS_AUTH_TOKEN_EVICT_TOTAL
                .with_label_values(&[if evicted {
                    "evicted"
                } else {
                    "already_rotated"
                }])
                .inc(),
            RuntimeEvent::WsStale { stage } => BYPASS_WS_POOL_STALE_TOTAL
                .with_label_values(&[stage.label()])
                .inc(),
            RuntimeEvent::WsStaleRetry { result } => BYPASS_WS_STALE_RETRY_TOTAL
                .with_label_values(&[result.label()])
                .inc(),
            RuntimeEvent::BudgetExhausted { kind } => BYPASS_RETRY_BUDGET_EXHAUSTED_TOTAL
                .with_label_values(&[kind.label()])
                .inc(),
        }
    }
}

fn status_class(status: u16) -> &'static str {
    match status {
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "0",
    }
}

pub struct UpstreamMetricsRecorder;

impl UpstreamMetricsRecorder {
    pub fn record_response(method: &str, status: reqwest::StatusCode, result: &str) {
        let class = status_class(status.as_u16());
        BYPASS_UPSTREAM_REQUESTS_TOTAL
            .with_label_values(&[method, class, result])
            .inc();
    }

    pub fn record_failure(method: &str, result: &str) {
        BYPASS_UPSTREAM_REQUESTS_TOTAL
            .with_label_values(&[method, "0", result])
            .inc();
        BYPASS_UPSTREAM_ERRORS_TOTAL
            .with_label_values(&[method, result])
            .inc();
    }

    pub fn record_timeout(method: &str) {
        Self::record_failure(method, "timeout");
    }

    pub fn record_transport_error(method: &str) {
        Self::record_failure(method, "transport");
    }

    pub fn record_ssrf(reason: &str) {
        let r = if reason.is_empty() { "blocked" } else { reason };
        // 低基数 reason：截断/归一化
        let reason_label = if r.contains("private") || r.contains("loopback") || r.contains("127.")
        {
            "private_ip"
        } else if r.contains("dns") || r.contains("resolve") {
            "dns"
        } else if r.contains("scheme") || r.contains("protocol") {
            "scheme"
        } else {
            "other"
        };
        BYPASS_SSRF_BLOCK_TOTAL
            .with_label_values(&[reason_label])
            .inc();
    }

    pub fn record_token_fetch(result: &str) {
        BYPASS_AUTH_TOKEN_FETCH_TOTAL
            .with_label_values(&[result])
            .inc();
    }

    pub fn record_config_cache(result: &str) {
        BYPASS_CONFIG_CACHE_TOTAL.with_label_values(&[result]).inc();
    }

    pub fn observe_duration(method: &str, result: &str, seconds: f64) {
        BYPASS_UPSTREAM_DURATION_SECONDS
            .with_label_values(&[method, result])
            .observe(seconds);
    }
}

#[cfg(test)]
mod observer_tests {
    use super::*;
    use mazbri_core::AdapterKind;
    use mazbri_upstream_runtime::AuthRejectTrigger;
    use mazbri_upstream_runtime::observe::{RetryKind, RetryOutcome, WsStaleStage};

    #[test]
    fn events_increment_closed_set_counters() {
        let rejected = || {
            BYPASS_UPSTREAM_AUTH_REJECTED_TOTAL
                .with_label_values(&["jsonrpc", "jsonrpc_code"])
                .get()
        };
        let retry_ok = || {
            BYPASS_UPSTREAM_AUTH_RETRY_TOTAL
                .with_label_values(&["jsonrpc", "ok"])
                .get()
        };
        let evicted = || {
            BYPASS_AUTH_TOKEN_EVICT_TOTAL
                .with_label_values(&["evicted"])
                .get()
        };
        let stale = || {
            BYPASS_WS_POOL_STALE_TOTAL
                .with_label_values(&["send"])
                .get()
        };
        let stale_ok = || BYPASS_WS_STALE_RETRY_TOTAL.with_label_values(&["ok"]).get();
        let budget = || {
            BYPASS_RETRY_BUDGET_EXHAUSTED_TOTAL
                .with_label_values(&["ws_stale"])
                .get()
        };
        let before = [
            rejected(),
            retry_ok(),
            evicted(),
            stale(),
            stale_ok(),
            budget(),
        ];
        let obs = BypassRuntimeObserver;
        obs.on_event(RuntimeEvent::AuthRejected {
            adapter: AdapterKind::Jsonrpc,
            trigger: AuthRejectTrigger::JsonRpcCode(-32001),
        });
        obs.on_event(RuntimeEvent::AuthRetry {
            adapter: AdapterKind::Jsonrpc,
            result: RetryOutcome::Ok,
        });
        obs.on_event(RuntimeEvent::TokenEvict { evicted: true });
        obs.on_event(RuntimeEvent::WsStale {
            stage: WsStaleStage::Send,
        });
        obs.on_event(RuntimeEvent::WsStaleRetry {
            result: RetryOutcome::Ok,
        });
        obs.on_event(RuntimeEvent::BudgetExhausted {
            kind: RetryKind::WsStale,
        });
        let after = [
            rejected(),
            retry_ok(),
            evicted(),
            stale(),
            stale_ok(),
            budget(),
        ];
        for (b, a) in before.iter().zip(after.iter()) {
            assert!(
                (a - b - 1.0).abs() < f64::EPSILON,
                "before={before:?} after={after:?}"
            );
        }
    }
}
