//! 管理端统一审计：路由映射 + 请求级中间件 + 业务事件合并 + 异步批量落库。

mod config;
mod ip;
mod middleware;
mod redact;
pub mod route_map;
pub mod scope;
mod service;
mod settings;
pub mod sink;
mod types;

pub use config::AuditConfig;
pub use ip::{TrustedProxies, extract_client_ip, init_trusted_proxies, resolve_client_ip};
pub use middleware::{apply_audit_layers, audit_middleware};
pub use redact::mask_unknown_username;
pub use scope::{
    AuditErrorInfo, AuditScope, add_detail, mark_security_event, mask_attempted_username,
    set_actor, set_attempted_username, set_failure_reason,
};
pub use service::{
    ACTION_LABELS, AuditService, RESOURCE_TYPE_LABELS, api_key_audit, config_center_audit,
    create_audit_log, tenant_audit, user_audit,
};
pub use settings::AuditSettingsCache;
pub use sink::{AuditWriterHandle, register_audit_metrics};
pub use types::{
    ActorType, AuditAction, AuditLogQuery, AuditLogRequest, AuditRecord, AuditResourceType,
    AuditStatus,
};
