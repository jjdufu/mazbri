//! 管理端统一审计：路由映射 + 请求级中间件 + 业务事件合并 + 异步批量落库。

pub mod actor_header;
mod config;
mod ip;
mod middleware;
pub mod route_map;
pub mod scope;
mod service;
mod settings;
mod types;

// 记录模型、脱敏与异步写入已抽到公共 crate `mazbri-audit`，保留原模块路径以减少改动面。
use mazbri_audit::redact;
pub use mazbri_audit::sink;

pub use config::AuditConfig;
pub use ip::{TrustedProxies, extract_client_ip, init_trusted_proxies, resolve_client_ip};
pub use middleware::{apply_audit_layers, audit_middleware};
pub use redact::mask_unknown_username;
pub use scope::{
    AuditErrorInfo, AuditScope, add_detail, mark_security_event, mask_attempted_username,
    set_actor, set_attempted_username, set_failure_reason,
};
pub use service::{
    ACTION_LABELS, AuditService, EXTERNAL_SERVICE_ACTIONS, RESOURCE_TYPE_LABELS, api_key_audit,
    config_center_audit, create_audit_log, tenant_audit, user_audit,
};
pub use settings::AuditSettingsCache;
pub use sink::{AuditWriterHandle, register_audit_metrics};
pub use types::{
    ActorType, AuditAction, AuditLogQuery, AuditLogRequest, AuditRecord, AuditResourceType,
    AuditStatus,
};
