//! `ClickHouse` `RowBinary` wire types.
//!
//! These structs mirror `ClickHouse` column types exactly (widths and nullability).
//! Domain models in `crate::models` stay chrono/API-friendly; convert at the boundary.

use crate::models::{AuditLogEntry, RequestLogEntry};
use chrono::{DateTime, TimeZone, Utc};
use mazbri_core::num::SaturatingFrom;
use serde::Deserialize;

/// `DateTime64(9)` → unix epoch nanoseconds.
#[inline]
fn datetime64_ns_to_utc(ns: i64) -> DateTime<Utc> {
    let secs = ns.div_euclid(1_000_000_000);
    let nanos = u32::saturating_from(ns.rem_euclid(1_000_000_000));
    Utc.timestamp_opt(secs, nanos)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().expect("epoch"))
}

/// `DateTime` → unix epoch seconds (u32 in `RowBinary`).
#[inline]
fn datetime_secs_to_utc(secs: u32) -> DateTime<Utc> {
    Utc.timestamp_opt(i64::from(secs), 0)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().expect("epoch"))
}

#[derive(Debug, Clone, clickhouse::Row, Deserialize)]
pub struct RequestLogRow {
    /// DateTime64(9) nanoseconds
    pub timestamp: i64,
    pub service: String,
    pub host: String,
    pub instance_id: String,
    pub event_target: String,
    pub message: String,
    pub request_id: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: String,
    pub tenant_id: String,
    pub tenant_code: String,
    pub product_id: String,
    pub supplier_id: String,
    pub supplier_code: String,
    pub channel_id: String,
    pub channel_code: String,
    pub attempt_order: u32,
    pub method: String,
    pub protocol: String,
    pub host_header: String,
    pub path: String,
    pub route_template: String,
    pub status_code: u16,
    pub duration_ms: u64,
    pub upstream_host: String,
    pub upstream_status_code: u16,
    pub upstream_service_time_ms: u64,
    pub dependency_wait_ms: u64,
    pub internal_ms: u64,
    pub post_response_ms: u64,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub client_ip: String,
    pub xff: String,
    pub user_agent: String,
    pub cache_hit: bool,
    pub fallback_used: bool,
    pub retry_count: u32,
    pub error_code: String,
    pub error_stage: String,
    pub response_flags: String,
    pub log_file: String,
    pub raw: String,
}

impl From<RequestLogRow> for RequestLogEntry {
    fn from(row: RequestLogRow) -> Self {
        Self {
            timestamp: datetime64_ns_to_utc(row.timestamp),
            service: row.service,
            host: row.host,
            instance_id: row.instance_id,
            event_target: row.event_target,
            message: row.message,
            request_id: row.request_id,
            trace_id: row.trace_id,
            span_id: row.span_id,
            parent_span_id: row.parent_span_id,
            tenant_id: row.tenant_id,
            tenant_code: row.tenant_code,
            product_id: row.product_id,
            supplier_id: row.supplier_id,
            supplier_code: row.supplier_code,
            channel_id: row.channel_id,
            channel_code: row.channel_code,
            attempt_order: row.attempt_order,
            method: row.method,
            protocol: row.protocol,
            host_header: row.host_header,
            path: row.path,
            route_template: row.route_template,
            status_code: row.status_code,
            duration_ms: row.duration_ms,
            upstream_host: row.upstream_host,
            upstream_status_code: row.upstream_status_code,
            upstream_service_time_ms: row.upstream_service_time_ms,
            dependency_wait_ms: row.dependency_wait_ms,
            internal_ms: row.internal_ms,
            post_response_ms: row.post_response_ms,
            request_bytes: row.request_bytes,
            response_bytes: row.response_bytes,
            client_ip: row.client_ip,
            xff: row.xff,
            user_agent: row.user_agent,
            cache_hit: row.cache_hit,
            fallback_used: row.fallback_used,
            retry_count: row.retry_count,
            error_code: row.error_code,
            error_stage: row.error_stage,
            response_flags: row.response_flags,
            log_file: row.log_file,
            raw: row.raw,
        }
    }
}

#[derive(Debug, Clone, clickhouse::Row, Deserialize)]
pub struct AuditLogRow {
    /// DateTime64(9) nanoseconds
    pub timestamp: i64,
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

impl From<AuditLogRow> for AuditLogEntry {
    fn from(row: AuditLogRow) -> Self {
        Self {
            timestamp: datetime64_ns_to_utc(row.timestamp),
            admin_id: row.admin_id,
            admin_username: row.admin_username,
            action: row.action,
            resource_type: row.resource_type,
            resource_id: row.resource_id,
            description: row.description,
            ip_address: row.ip_address,
            user_agent: row.user_agent,
            request_id: row.request_id,
            old_value: row.old_value,
            new_value: row.new_value,
        }
    }
}

#[derive(Debug, Clone, clickhouse::Row, Deserialize)]
pub struct AuditAnalyticsRow {
    pub period: String,
    pub action: String,
    pub resource_type: String,
    pub count: u64,
}

#[derive(Debug, Default, clickhouse::Row, Deserialize)]
pub struct SummaryRow {
    pub total: u64,
    pub avg_duration_ms: f64,
    pub avg_upstream_service_time_ms: f64,
    pub avg_dependency_wait_ms: f64,
    pub avg_internal_ms: f64,
    pub avg_post_response_ms: f64,
}

#[derive(Debug, clickhouse::Row, Deserialize)]
pub struct TimeBucketRow {
    /// `DateTime` (seconds since epoch)
    pub time: u32,
    pub count: u64,
}

impl TimeBucketRow {
    #[must_use]
    pub fn into_time_bucket(self) -> crate::greptimedb::TimeBucket {
        crate::greptimedb::TimeBucket {
            time: datetime_secs_to_utc(self.time),
            count: self.count,
        }
    }
}

#[derive(Debug, clickhouse::Row, Deserialize)]
pub struct GroupedCountRow {
    pub key: String,
    pub cnt: u64,
}

#[derive(clickhouse::Row, Deserialize)]
pub struct TotalRow {
    pub total: u64,
}

#[derive(Debug, Clone, clickhouse::Row, Deserialize, serde::Serialize)]
pub struct UpstreamHourlyUsageRow {
    pub supplier_id: String,
    pub supplier_code: String,
    pub channel_id: String,
    pub channel_code: String,
    pub total_calls: u64,
    pub success_calls: u64,
    pub failed_calls: u64,
    pub total_payload_bytes: u64,
    pub total_latency_ms: u64,
}
