use chrono::{DateTime, Utc};
use mazbri_analytics::greptimedb::AuditLogEntry;
use serde::{Deserialize, Serialize};
fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    20
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditQueryRequest {
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default)]
    pub admin_id: Option<i64>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub resource_type: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct AuditLogItem {
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
impl From<AuditLogEntry> for AuditLogItem {
    fn from(entry: AuditLogEntry) -> Self {
        Self {
            timestamp: entry.timestamp,
            admin_id: entry.admin_id,
            admin_username: entry.admin_username,
            action: entry.action,
            resource_type: entry.resource_type,
            resource_id: entry.resource_id,
            description: entry.description,
            ip_address: entry.ip_address,
            user_agent: entry.user_agent,
            request_id: entry.request_id,
            old_value: entry.old_value,
            new_value: entry.new_value,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct AuditQueryResponse {
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
    pub data: Vec<AuditLogItem>,
}
fn default_group_by() -> String {
    "day".to_string()
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditAnalyticsRequest {
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default)]
    pub admin_id: Option<i64>,
    #[serde(default = "default_group_by")]
    pub group_by: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct AuditAnalyticsTimelineItem {
    pub period: String,
    pub total_actions: u64,
    pub by_action: std::collections::HashMap<String, u64>,
    pub by_resource: std::collections::HashMap<String, u64>,
}
#[derive(Debug, Clone, Serialize)]
pub struct AuditAnalyticsResponse {
    pub period_start: String,
    pub period_end: String,
    pub total_actions: u64,
    pub by_action: std::collections::HashMap<String, u64>,
    pub by_resource: std::collections::HashMap<String, u64>,
    pub timeline: Vec<AuditAnalyticsTimelineItem>,
}
pub fn parse_time_string(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if let Ok(ts) = s.parse::<i64>()
        && let Some(dt) = DateTime::from_timestamp(ts, 0)
    {
        return Some(dt);
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    let formats = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d"];
    for fmt in formats {
        if let Ok((naive_dt, _)) = chrono::NaiveDateTime::parse_and_remainder(s, fmt) {
            return Some(naive_dt.and_utc());
        }
        if let Ok((naive_date, _)) = chrono::NaiveDate::parse_and_remainder(s, fmt)
            && let Some(naive_dt) = naive_date.and_hms_opt(0, 0, 0)
        {
            return Some(naive_dt.and_utc());
        }
    }
    None
}
