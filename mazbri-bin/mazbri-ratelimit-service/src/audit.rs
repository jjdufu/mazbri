//! 统一审计（`mazbri_admin.audit_logs`，`service = 'ratelimit'`）：记录 gateway 对控制命令的确认。
//!
//! 只在 `[rate_limit.audit]` 启用且配置了 `[rate_limit.audit.database]`（写入账号
//! `mazbri_audit_writer`，仅有 `audit_logs` 的 INSERT 权限）时生效；否则不产生记录。
use mazbri_audit::record::{ActorType, AuditRecord, AuditStatus, SERVICE_RATELIMIT};
use mazbri_audit::sink::{
    AuditSink, AuditStore, AuditWriterHandle, FallbackWriter, PgAuditStore, SinkOptions,
};
use std::path::PathBuf;
use std::sync::Arc;

/// 控制命令确认动作。
pub const ACTION_COMMAND_ACK: &str = "ratelimit.command_ack";
/// 资源类型：限流。
pub const RESOURCE_TYPE_RATELIMIT: &str = "ratelimit";
/// 单条记录最多保留的命令 ID 数（其余只计数）。
const MAX_RECORDED_IDS: usize = 50;

/// 启动审计写入器；未启用或未配置数据库时返回 `None`。
///
/// # Errors
///
/// 数据库配置无法构建连接参数时返回错误。
pub fn start(
    config: &crate::config::AuditConfig,
) -> anyhow::Result<Option<(AuditSink, AuditWriterHandle)>> {
    if !config.enabled {
        return Ok(None);
    }
    let Some(database) = &config.database else {
        tracing::warn!(
            "rate_limit.audit.database is not configured; control command acks are not written to the unified audit log"
        );
        return Ok(None);
    };
    let pool = mazbri_core::create_lazy_pool(database, None)?;
    let store: Arc<dyn AuditStore> = Arc::new(PgAuditStore::new(pool));
    let fallback = Arc::new(FallbackWriter::new(
        config.fallback_file.as_ref().map(PathBuf::from),
    ));
    tracing::info!("ratelimit unified audit initialized");
    Ok(Some(AuditSink::start(
        store,
        fallback,
        SinkOptions::default(),
    )))
}

/// 构造控制命令确认的系统审计记录（已脱敏）。
///
/// `outcome`：成功时为确认条数，失败时为 `(failure_reason, error_message)`。
#[must_use]
pub fn command_ack_record(
    gateway_id: &str,
    command_ids: &[String],
    outcome: Result<i64, (&str, &str)>,
) -> AuditRecord {
    let mut record = AuditRecord::new(SERVICE_RATELIMIT, &ActorType::System, ACTION_COMMAND_ACK);
    record.resource_type = Some(RESOURCE_TYPE_RATELIMIT.to_string());
    record.resource_id = Some(gateway_id.to_string());
    let recorded: Vec<&String> = command_ids.iter().take(MAX_RECORDED_IDS).collect();
    let mut details = serde_json::json!({
        "gateway_id": gateway_id,
        "command_ids": recorded,
        "requested": command_ids.len(),
        "command_ids_truncated": command_ids.len() > MAX_RECORDED_IDS,
    });
    match outcome {
        Ok(acknowledged) => {
            details["acknowledged"] = serde_json::json!(acknowledged);
        }
        Err((reason, message)) => {
            record.status = AuditStatus::Failure;
            record.failure_reason = Some(reason.to_string());
            record.error_message = Some(message.to_string());
        }
    }
    record.details = Some(details);
    record.sanitize();
    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_audit::sink::testing::MemoryStore;

    fn ids(n: usize) -> Vec<String> {
        (0..n)
            .map(|i| format!("00000000-0000-0000-0000-{i:012}"))
            .collect()
    }

    #[test]
    fn success_record_is_system_ratelimit_event() {
        let record = command_ack_record("gw-1", &ids(2), Ok(2));
        assert_eq!(record.service, "ratelimit");
        assert_eq!(record.actor_type, "system");
        assert!(record.user_id.is_none());
        assert_eq!(record.action, ACTION_COMMAND_ACK);
        assert_eq!(record.resource_type.as_deref(), Some("ratelimit"));
        assert_eq!(record.resource_id.as_deref(), Some("gw-1"));
        assert_eq!(record.status, AuditStatus::Success);
        let details = record.details.expect("details");
        assert_eq!(details["acknowledged"], 2);
        assert_eq!(details["requested"], 2);
        assert_eq!(details["command_ids_truncated"], false);
    }

    #[test]
    fn failure_record_and_truncated_ids() {
        let record = command_ack_record("gw-1", &ids(60), Err(("db_error", "database error")));
        assert_eq!(record.status, AuditStatus::Failure);
        assert_eq!(record.failure_reason.as_deref(), Some("db_error"));
        let details = record.details.expect("details");
        assert_eq!(details["command_ids"].as_array().map(Vec::len), Some(50));
        assert_eq!(details["requested"], 60);
        assert_eq!(details["command_ids_truncated"], true);
        assert!(details.get("acknowledged").is_none());
    }

    #[tokio::test]
    async fn disabled_or_unconfigured_does_not_start() {
        let disabled = crate::config::AuditConfig {
            enabled: false,
            ..crate::config::AuditConfig::default()
        };
        assert!(start(&disabled).expect("start").is_none());
        let no_db = crate::config::AuditConfig::default();
        assert!(start(&no_db).expect("start").is_none());
    }

    #[tokio::test]
    async fn sink_writes_ack_record() {
        let store = Arc::new(MemoryStore::default());
        let (sink, handle) = AuditSink::start(
            store.clone(),
            Arc::new(FallbackWriter::new(None)),
            SinkOptions::default(),
        );
        sink.enqueue(command_ack_record("gw-2", &ids(1), Ok(1)));
        handle.shutdown(std::time::Duration::from_secs(2)).await;
        let rows = store.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].service, "ratelimit");
    }
}
