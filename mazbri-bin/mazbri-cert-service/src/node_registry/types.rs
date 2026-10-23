use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RegisteredNode {
    pub id: Uuid,
    pub service_name: String,
    pub ip: String,
    pub port: i32,
    pub started_at: DateTime<Utc>,
    pub config_version: i32,
    pub config_md5: Option<String>,
    pub health_check_url: Option<String>,
    pub last_heartbeat_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterNodeData {
    pub service_name: String,
    pub ip: String,
    pub port: u16,
    pub started_at: i64,
    pub config_version: i32,
    pub config_md5: String,
    pub health_check_url: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatData {
    pub service_name: String,
    pub config_version: i32,
}
