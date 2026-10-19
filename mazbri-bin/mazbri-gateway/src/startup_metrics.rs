use mazbri_core::metrics::{MetricsBuilder, REGISTRY, register_collector_once};
use prometheus::IntGauge;
use std::sync::LazyLock;
pub static GATEWAY_STARTUP_READY: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge(
        "gateway_startup_ready",
        "Whether gateway startup warmup has completed (1=ready, 0=not ready)",
    )
});
pub fn register_gateway_startup_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(GATEWAY_STARTUP_READY.clone()))
}
pub fn set_gateway_startup_ready(ready: bool) {
    GATEWAY_STARTUP_READY.set(if ready { 1 } else { 0 });
}
