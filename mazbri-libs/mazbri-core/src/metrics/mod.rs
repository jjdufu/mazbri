pub mod builders;
pub mod cbpro;
pub mod defaults;
pub mod dep_client;
pub mod health;
pub mod otlp;
pub mod process;
pub mod recorders;
pub mod registry;
pub mod rpc_client;
pub mod runtime;
pub mod service;
pub use builders::MetricsBuilder;
pub use cbpro::{
    CBPRO_CONFIG_CHANGES_TOTAL, CBPRO_CURRENT_STATE, CBPRO_ERROR_RATE_PERCENTAGE,
    CBPRO_PROTECTION_TRIGGERS_TOTAL, CBPRO_STATE_TRANSITIONS_TOTAL, CbProMetricsRecorder,
    register_cbpro_metrics,
};
pub use defaults::{
    ACTIVE_CONNECTIONS, DB_QUERY_TOTAL, HTTP_REQUEST_DURATION_SECONDS, HTTP_REQUESTS_TOTAL,
    REGISTRY, register_metrics,
};
pub use health::{
    BoxedConfigCenterCheck, BoxedReadyCheck, CONFIG_CENTER_CONNECTED, CertExpiryInfoProvider,
    DB_POOL_ACTIVE_CONNECTIONS, DB_POOL_IDLE_CONNECTIONS, DbPoolInfo, HealthMetricsUpdater,
    LateDbPool, RPC_HEALTH_CHECK, RPC_READY, TLS_CERT_EXPIRY_TIMESTAMP, record_db_pool_metrics,
    register_health_metrics, set_rpc_ready,
};
pub use otlp::{
    OTLP_SPANS_DROPPED_TOTAL, SpanDropReason, record_spans_dropped, register_otlp_metrics,
};
pub use process::{register_process_metrics, spawn_process_metrics};
pub use recorders::{CounterRecorder, HistogramRecorder, MetricsRecorder};
pub use registry::register_collector_once;
pub use runtime::{register_runtime_metrics, spawn_runtime_metrics};
pub use service::{BUILD_GIT_SHA, BUILD_INFO, BUILD_VERSION, init_service_metrics};

pub use dep_client::{
    DEP_CLIENT_DURATION_SECONDS, DEP_CLIENT_ERRORS_TOTAL, current_service, observe_dep,
    observe_dep_duration, register_dep_client_metrics, set_current_service,
};
pub use rpc_client::{
    RPC_CLIENT_DURATION_SECONDS, RPC_CLIENT_REQUESTS_TOTAL, RpcMetricsChannel, RpcMetricsStub,
    register_rpc_client_metrics, spawn_rpc_client,
};
