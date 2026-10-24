//! Tokio runtime metrics + scheduler-lag probe.
//!
//! Sampled once per second from `Handle::current().metrics()` (stable API only, no
//! `tokio_unstable`). The lag probe sleeps a fixed interval and records how late it wakes:
//! a busy/blocked runtime shows up as lag, which separates "our process stalled" from
//! "a dependency (PG/bypass) was slow" when explaining latency spikes.
use super::{MetricsBuilder, register_collector_once};
use crate::REGISTRY;
use prometheus::{Gauge, Histogram, HistogramOpts, IntCounter, IntGauge, Opts};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

/// Probe sleep; lag = actual wake-up delay beyond this.
const PROBE_INTERVAL: Duration = Duration::from_millis(100);
/// Lag above this is logged at WARN (with timestamp) as stall evidence.
const LAG_WARN: Duration = Duration::from_millis(50);

pub static TOKIO_WORKERS: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge("tokio_workers", "Number of tokio runtime worker threads")
});
pub static TOKIO_ALIVE_TASKS: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge("tokio_alive_tasks", "Number of alive tokio tasks")
});
pub static TOKIO_GLOBAL_QUEUE_DEPTH: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge(
        "tokio_global_queue_depth",
        "Tasks waiting in the tokio global (injection) queue",
    )
});
pub static TOKIO_WORKER_BUSY_SECONDS: LazyLock<Gauge> = LazyLock::new(|| {
    Gauge::with_opts(Opts::new(
        "tokio_worker_busy_seconds_total",
        "Sum over workers of time spent busy (monotonic; rate() = busy workers)",
    ))
    .expect("tokio_worker_busy_seconds_total metric")
});
pub static TOKIO_WORKER_PARK_TOTAL: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge(
        "tokio_worker_park_total",
        "Sum over workers of park count (monotonic)",
    )
});
pub static TOKIO_SCHEDULER_LAG_SECONDS: LazyLock<Histogram> = LazyLock::new(|| {
    Histogram::with_opts(
        HistogramOpts::new(
            "tokio_scheduler_lag_seconds",
            "Wake-up delay of a 100ms sleep probe beyond its deadline",
        )
        .buckets(vec![
            0.001, 0.002, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5,
        ]),
    )
    .expect("tokio_scheduler_lag_seconds metric")
});
pub static TOKIO_SCHEDULER_LAG_STALLS_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::with_opts(Opts::new(
        "tokio_scheduler_lag_stalls_total",
        "Probe wake-ups later than 50ms",
    ))
    .expect("tokio_scheduler_lag_stalls_total metric")
});

/// # Errors
///
/// Prometheus registration failed.
pub fn register_runtime_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(TOKIO_WORKERS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(TOKIO_ALIVE_TASKS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(TOKIO_GLOBAL_QUEUE_DEPTH.clone()))?;
    register_collector_once(&REGISTRY, Box::new(TOKIO_WORKER_BUSY_SECONDS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(TOKIO_WORKER_PARK_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(TOKIO_SCHEDULER_LAG_SECONDS.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(TOKIO_SCHEDULER_LAG_STALLS_TOTAL.clone()),
    )?;
    Ok(())
}

fn sample(handle: &tokio::runtime::Handle) {
    let m = handle.metrics();
    let workers = m.num_workers();
    let mut busy = Duration::ZERO;
    let mut parks: u64 = 0;
    for w in 0..workers {
        busy += m.worker_total_busy_duration(w);
        parks = parks.saturating_add(m.worker_park_count(w));
    }
    TOKIO_WORKERS.set(i64::try_from(workers).unwrap_or(i64::MAX));
    TOKIO_ALIVE_TASKS.set(i64::try_from(m.num_alive_tasks()).unwrap_or(i64::MAX));
    TOKIO_GLOBAL_QUEUE_DEPTH.set(i64::try_from(m.global_queue_depth()).unwrap_or(i64::MAX));
    TOKIO_WORKER_BUSY_SECONDS.set(busy.as_secs_f64());
    TOKIO_WORKER_PARK_TOTAL.set(i64::try_from(parks).unwrap_or(i64::MAX));
}

/// Register and start the sampler (1s) and the lag probe (100ms) on the current runtime.
/// Must be called from within a tokio runtime. Registration errors are logged, not fatal.
pub fn spawn_runtime_metrics(service: &'static str) {
    if let Err(error) = register_runtime_metrics() {
        tracing::warn!(%error, "tokio runtime metrics registration failed");
    }
    let handle = tokio::runtime::Handle::current();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            sample(&handle);
        }
    });
    tokio::spawn(async move {
        loop {
            let start = Instant::now();
            tokio::time::sleep(PROBE_INTERVAL).await;
            let lag = start.elapsed().saturating_sub(PROBE_INTERVAL);
            TOKIO_SCHEDULER_LAG_SECONDS.observe(lag.as_secs_f64());
            if lag > LAG_WARN {
                TOKIO_SCHEDULER_LAG_STALLS_TOTAL.inc();
                tracing::warn!(
                    service,
                    lag_ms = u64::try_from(lag.as_millis()).unwrap_or(u64::MAX),
                    "tokio scheduler lag above 50ms (runtime stalled or CPU-throttled)"
                );
            }
        }
    });
}
