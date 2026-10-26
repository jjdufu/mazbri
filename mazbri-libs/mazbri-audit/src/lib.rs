//! 统一审计公共组件：记录模型、脱敏截断、异步批量写入（有界队列 + 重试 + JSONL 兜底）。
//!
//! 由 mazbri-mgmt 的审计中间件使用；其它服务（cert、ratelimit 等）也通过它写入
//! `mazbri_admin.audit_logs`。

pub mod record;
pub mod redact;
pub mod sink;

pub use record::{ActorType, AuditRecord, AuditStatus};
pub use sink::{
    AuditSink, AuditStore, AuditWriterHandle, FallbackWriter, PgAuditStore, SinkOptions,
};
