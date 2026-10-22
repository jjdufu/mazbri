use mazbri_core::metrics::{REGISTRY, register_collector_once};
use prometheus::{Gauge, HistogramOpts, HistogramVec, IntCounterVec, Opts};
use std::sync::LazyLock;

pub static LOCAL_RATELIMIT_DECISIONS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new(
            "gateway_ratelimit_decisions_total",
            "Total gateway rate limit decisions",
        ),
        &["decision", "source", "degraded", "reason"],
    )
    .expect("valid metric definition for gateway ratelimit decisions counter")
});

pub static LOCAL_RATELIMIT_CHECK_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    HistogramVec::new(
        HistogramOpts::new(
            "gateway_ratelimit_check_duration_seconds",
            "Gateway local rate limit check duration",
        ),
        &["decision", "degraded"],
    )
    .expect("valid metric definition for gateway ratelimit check duration")
});

pub static LOCAL_RATELIMIT_REFRESH_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new(
            "gateway_ratelimit_snapshot_refresh_total",
            "Gateway local rate limit snapshot refresh results",
        ),
        &["result"],
    )
    .expect("valid metric definition for gateway ratelimit snapshot refresh")
});

pub static LOCAL_RATELIMIT_USAGE_FLUSH_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new(
            "gateway_ratelimit_usage_flush_total",
            "Gateway local rate limit usage flush results",
        ),
        &["result"],
    )
    .expect("valid metric definition for gateway ratelimit usage flush")
});

pub static LOCAL_RATELIMIT_SNAPSHOT_AGE_SECONDS: LazyLock<Gauge> = LazyLock::new(|| {
    Gauge::new(
        "gateway_ratelimit_snapshot_age_seconds",
        "Age in seconds of the oldest cached tenant rate-limit policy snapshot",
    )
    .expect("valid metric definition for gateway ratelimit snapshot age")
});

pub static LOCAL_RATELIMIT_CONTROL_COMMANDS_APPLIED_TOTAL: LazyLock<IntCounterVec> =
    LazyLock::new(|| {
        IntCounterVec::new(
            Opts::new(
                "gateway_ratelimit_control_commands_applied_total",
                "Control commands applied by the gateway local engine",
            ),
            &["type"],
        )
        .expect("valid metric definition for gateway ratelimit control commands applied")
    });

pub static LOCAL_RATELIMIT_USAGE_BUFFER_DROPPED_TOTAL: LazyLock<IntCounterVec> =
    LazyLock::new(|| {
        IntCounterVec::new(
            Opts::new(
                "gateway_ratelimit_usage_buffer_dropped_total",
                "Usage delta records dropped because the local buffer was full",
            ),
            &["reason"],
        )
        .expect("valid metric definition for gateway ratelimit usage buffer drops")
    });

pub static CHANNEL_RATELIMIT_DECISIONS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new(
            "gateway_channel_ratelimit_decisions_total",
            "Total gateway channel (cross-tenant) rate limit decisions",
        ),
        &["decision", "reason", "mode"],
    )
    .expect("valid metric definition for gateway channel ratelimit decisions counter")
});

pub static CHANNEL_RATELIMIT_CHECK_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    HistogramVec::new(
        HistogramOpts::new(
            "gateway_channel_ratelimit_check_duration_seconds",
            "Gateway channel rate limit check duration",
        ),
        &["decision", "mode"],
    )
    .expect("valid metric definition for gateway channel ratelimit check duration")
});

pub static CHANNEL_RATELIMIT_QUEUE_WAIT_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    HistogramVec::new(
        HistogramOpts::new(
            "gateway_channel_ratelimit_queue_wait_seconds",
            "Time spent waiting in channel rate-limit queue mode",
        )
        .buckets(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ]),
        &["result"],
    )
    .expect("valid metric definition for gateway channel ratelimit queue wait")
});

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_local_ratelimit_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(LOCAL_RATELIMIT_DECISIONS_TOTAL.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(LOCAL_RATELIMIT_CHECK_DURATION_SECONDS.clone()),
    )?;
    register_collector_once(&REGISTRY, Box::new(LOCAL_RATELIMIT_REFRESH_TOTAL.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(LOCAL_RATELIMIT_USAGE_FLUSH_TOTAL.clone()),
    )?;
    register_collector_once(
        &REGISTRY,
        Box::new(LOCAL_RATELIMIT_SNAPSHOT_AGE_SECONDS.clone()),
    )?;
    register_collector_once(
        &REGISTRY,
        Box::new(LOCAL_RATELIMIT_CONTROL_COMMANDS_APPLIED_TOTAL.clone()),
    )?;
    register_collector_once(
        &REGISTRY,
        Box::new(LOCAL_RATELIMIT_USAGE_BUFFER_DROPPED_TOTAL.clone()),
    )?;
    register_collector_once(
        &REGISTRY,
        Box::new(CHANNEL_RATELIMIT_DECISIONS_TOTAL.clone()),
    )?;
    register_collector_once(
        &REGISTRY,
        Box::new(CHANNEL_RATELIMIT_CHECK_DURATION_SECONDS.clone()),
    )?;
    register_collector_once(
        &REGISTRY,
        Box::new(CHANNEL_RATELIMIT_QUEUE_WAIT_SECONDS.clone()),
    )?;
    Ok(())
}
