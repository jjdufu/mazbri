mod admin_simple;
mod bootstrap;
mod health;
mod middleware;
mod registry_helpers;
mod shutdown;
mod tarpc_server;
pub use admin_simple::start_simple_admin_server;
use axum::{Router, routing::get};
pub use bootstrap::{
    BootstrapContext, ServiceBootstrap, ServiceInitOutput, ServiceRuntime, init_service_runtime,
    run_toml_config_service, start_admin_server,
};
pub use health::{HealthCheck, HealthResponse, liveness, readiness};
pub use middleware::{error_handler_middleware, metrics_middleware, request_id_middleware};
pub use registry_helpers::{
    TomlServiceConfigHandler, create_toml_config_handler, make_config_center_check,
};
pub use shutdown::{
    ShutdownHandle, create_shutdown_manager, wait_for_shutdown, wait_for_termination_signal,
};
pub fn create_base_router() -> Router {
    Router::new()
        .route("/health", get(liveness))
        .route("/ready", get(readiness))
}
pub use mazbri_tls::{CertExpiryInfo, ManagedCertHandle};
