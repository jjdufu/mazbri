pub use crate::models::RequestLogEntry;
use chrono::{DateTime, Utc};
use mazbri_core::num::SaturatingFrom;
use serde::{Deserialize, Deserializer, Serialize};

fn deserialize_i64_as_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<i64>::deserialize(deserializer)?;
    Ok(u64::saturating_from(opt.unwrap_or(0)))
}

fn deserialize_greptime_timestamp<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Number(n) => {
            let nanos = n
                .as_i64()
                .ok_or_else(|| serde::de::Error::custom("timestamp not i64"))?;
            let secs = nanos / 1_000_000_000;
            let nanos_part = u32::saturating_from(nanos % 1_000_000_000);
            DateTime::from_timestamp(secs, nanos_part)
                .ok_or_else(|| serde::de::Error::custom("invalid timestamp"))
        }
        serde_json::Value::String(s) => {
            if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
                return Ok(dt.with_timezone(&Utc));
            }
            if let Ok(dt) = s.parse::<DateTime<Utc>>() {
                return Ok(dt);
            }
            let formats = [
                "%Y-%m-%d %H:%M:%S",
                "%Y-%m-%d %H:%M:%S%.3f",
                "%Y-%m-%d %H:%M:%S%.6f",
                "%Y-%m-%d %H:%M:%S%.9f",
                "%Y-%m-%dT%H:%M:%S",
                "%Y-%m-%dT%H:%M:%S%.3f",
                "%Y-%m-%dT%H:%M:%S%.6f",
                "%Y-%m-%dT%H:%M:%S%.9f",
            ];
            for fmt in &formats {
                if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(&s, fmt) {
                    return Ok(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc));
                }
            }
            if let Ok(nd) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                && let Some(ndt) = nd.and_hms_opt(0, 0, 0)
            {
                return Ok(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc));
            }
            Err(serde::de::Error::custom(format!(
                "unexpected timestamp format: {s}"
            )))
        }
        _ => Err(serde::de::Error::custom("unexpected timestamp type")),
    }
}

#[derive(Debug, Clone)]
pub struct RequestLogQueryParams {
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    /// Free-text keyword (`q`).
    pub q: Option<String>,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub tenant_id: Option<String>,
    pub tenant_code: Option<String>,
    pub product_id: Option<String>,
    pub supplier_id: Option<String>,
    pub supplier_code: Option<String>,
    pub channel_id: Option<String>,
    pub channel_code: Option<String>,
    pub path: Option<String>,
    pub route_template: Option<String>,
    pub client_ip: Option<String>,
    pub upstream_host: Option<String>,
    pub upstream_status_code: Option<u16>,
    pub error_code: Option<String>,
    pub error_stage: Option<String>,
    pub response_flags: Option<String>,
    pub status_code: Option<u16>,
    /// Access-log writer scope (`request_logs.event_target`).
    pub event_target: Option<EventTarget>,
    /// Structured filters (already validated against `call_monitor` catalog).
    pub filters: Vec<crate::list_query::ListFilter>,
    /// When set, list/search only inside the latest N rows (event-target scoped), then apply filters.
    pub latest_universe_limit: Option<usize>,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct RequestLogQueryResult {
    pub logs: Vec<RequestLogEntry>,
    pub total: u64,
}

#[derive(Debug, Clone)]
pub struct RequestLogAggregationParams {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub group_by_route: bool,
    pub group_by_time: Option<String>,
    pub tenant_id: Option<String>,
    /// When set, only count rows of this access-log writer (e.g. gateway: customer volume).
    pub event_target: Option<EventTarget>,
}

/// `request_logs.event_target`: which writer produced the access-log row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventTarget {
    Gateway,
    Bypass,
    Billing,
}

impl EventTarget {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gateway => "gateway",
            Self::Bypass => "bypass",
            Self::Billing => "billing",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeBucket {
    #[serde(deserialize_with = "deserialize_greptime_timestamp")]
    pub time: DateTime<Utc>,
    #[serde(default, deserialize_with = "deserialize_i64_as_u64")]
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestLogAggregation {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_requests: u64,
    pub avg_duration_ms: f64,
    pub avg_upstream_service_time_ms: f64,
    pub avg_dependency_wait_ms: f64,
    pub avg_internal_ms: f64,
    pub avg_post_response_ms: f64,
    pub by_route: std::collections::HashMap<String, u64>,
    pub by_status: std::collections::HashMap<u16, u64>,
    pub by_time: Vec<TimeBucket>,
}
