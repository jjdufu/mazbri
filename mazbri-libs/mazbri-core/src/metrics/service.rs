//! Common per-service metrics: build info, process resources and tokio runtime.
use super::{
    process::spawn_process_metrics, register_collector_once, runtime::spawn_runtime_metrics,
};
use crate::REGISTRY;
use prometheus::{IntGaugeVec, Opts};
use std::sync::LazyLock;

/// Release version from the repository `VERSION` file (see `build.rs`).
pub const BUILD_VERSION: &str = env!("MAZBRI_VERSION");
/// Git commit the binary was built from (see `build.rs`).
pub const BUILD_GIT_SHA: &str = env!("MAZBRI_GIT_SHA");

pub static BUILD_INFO: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    IntGaugeVec::new(
        Opts::new(
            "mazbri_build_info",
            "Constant 1; labels carry the service name, release version and git commit",
        ),
        &["service", "version", "git_sha"],
    )
    .expect("mazbri_build_info metric")
});

/// One-call setup every service binary runs at startup: `mazbri_build_info`,
/// process resource sampling and tokio runtime metrics.
/// `service` is the canonical name (`ServiceName::as_str`, e.g. `mazbri-auth`).
/// Must be called from within a tokio runtime. Registration errors are logged, not fatal.
pub fn init_service_metrics(service: &'static str) {
    super::dep_client::set_current_service(service);
    match register_collector_once(&REGISTRY, Box::new(BUILD_INFO.clone())) {
        Ok(()) => BUILD_INFO
            .with_label_values(&[service, BUILD_VERSION, BUILD_GIT_SHA])
            .set(1),
        Err(error) => tracing::warn!(%error, "mazbri_build_info registration failed"),
    }
    super::rpc_client::register_rpc_client_metrics();
    super::dep_client::register_dep_client_metrics();
    super::otlp::register_otlp_metrics(service);
    spawn_process_metrics();
    spawn_runtime_metrics(service);
}
