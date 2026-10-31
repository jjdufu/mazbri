//! Service → dependency (pg/redis/kafka/ch/greptime) client RED metrics.
use super::{MetricsBuilder, register_collector_once};
use crate::REGISTRY;
use prometheus::{CounterVec, HistogramVec};
use std::sync::{LazyLock, OnceLock};
use std::time::{Duration, Instant};

static CURRENT_SERVICE: OnceLock<&'static str> = OnceLock::new();

/// Remember the process service name for dep metric labels.
/// Called from [`super::service::init_service_metrics`]. Idempotent.
pub fn set_current_service(service: &'static str) {
    let _ = CURRENT_SERVICE.set(service);
}

#[must_use]
pub fn current_service() -> &'static str {
    CURRENT_SERVICE.get().copied().unwrap_or("unknown")
}

pub static DEP_CLIENT_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "mazbri_dep_client_duration_seconds",
        "Dependency client call duration in seconds",
        vec![
            0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ],
        &["service", "dep", "op"],
    )
});

pub static DEP_CLIENT_ERRORS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "mazbri_dep_client_errors_total",
        "Dependency client call errors",
        &["service", "dep", "op"],
    )
});

/// Register dep client collectors (idempotent).
pub fn register_dep_client_metrics() {
    if let Err(error) =
        register_collector_once(&REGISTRY, Box::new(DEP_CLIENT_DURATION_SECONDS.clone()))
    {
        tracing::warn!(%error, "mazbri_dep_client_duration_seconds registration failed");
    }
    if let Err(error) =
        register_collector_once(&REGISTRY, Box::new(DEP_CLIENT_ERRORS_TOTAL.clone()))
    {
        tracing::warn!(%error, "mazbri_dep_client_errors_total registration failed");
    }
}

/// Record one dependency call. `ok` false increments the error counter.
pub fn observe_dep(dep: &'static str, op: &'static str, started: Instant, ok: bool) {
    observe_dep_duration(dep, op, started.elapsed(), ok);
}

/// Record one dependency call with an already-measured duration.
pub fn observe_dep_duration(dep: &'static str, op: &'static str, duration: Duration, ok: bool) {
    let service = current_service();
    DEP_CLIENT_DURATION_SECONDS
        .with_label_values(&[service, dep, op])
        .observe(duration.as_secs_f64());
    if !ok {
        DEP_CLIENT_ERRORS_TOTAL
            .with_label_values(&[service, dep, op])
            .inc();
    }
}

