mod admin_simple;
mod bootstrap;
mod middleware;
mod registry_helpers;
mod shutdown;
mod tarpc_server;
pub mod trace_layer;
pub use admin_simple::start_simple_admin_server;
pub use bootstrap::{
    BootstrapContext, ServiceInitOutput, ServiceRuntime, init_service_runtime,
    run_toml_config_service,
};
/// 供 `run_tarpc_server!` 宏展开使用：单个 tarpc 请求的响应任务属于可恢复范围。
pub use mazbri_core::panic_policy;
pub use middleware::{error_handler_middleware, metrics_middleware, request_id_middleware};
pub use shutdown::{ShutdownHandle, create_shutdown_manager, wait_for_termination_signal};
pub use trace_layer::http_trace_layer;
