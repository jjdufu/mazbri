use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Domain/API model for an audit log entry.
/// ClickHouse wire types live in `crate::clickhouse::types`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub timestamp: DateTime<Utc>,
    pub admin_id: i64,
    pub admin_username: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub description: String,
    pub ip_address: String,
    pub user_agent: String,
    pub request_id: String,
    pub old_value: String,
    pub new_value: String,
}
