//! 数据管道新鲜度：周期查询各存储关键表的 `now() - max(ts)`，导出
//! `mazbri_pipeline_freshness_seconds{store,table}`。
//!
//! 只扫最近 `LOOKBACK_SECS` 内的数据；窗口内无数据或查询失败时取上限值，
//! 让“管道停摆”与“查询不可用”都表现为新鲜度变差而不是指标消失。

use crate::clickhouse::ClickHouseClient;
use crate::greptimedb::GreptimeDBClient;
use mazbri_core::metrics::{REGISTRY, register_collector_once};
use mazbri_core::num::{i64_to_f64, u64_to_f64};
use prometheus::{GaugeVec, Opts};
use serde::Deserialize;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

const INTERVAL: Duration = Duration::from_secs(30);
/// 扫描窗口与新鲜度上限（秒）。
const LOOKBACK_SECS: u64 = 86_400;

pub static PIPELINE_FRESHNESS_SECONDS: LazyLock<GaugeVec> = LazyLock::new(|| {
    GaugeVec::new(
        Opts::new(
            "mazbri_pipeline_freshness_seconds",
            "Seconds since the newest row of a key pipeline table (capped at 86400)",
        ),
        &["store", "table"],
    )
    .expect("mazbri_pipeline_freshness_seconds")
});

/// # Errors
///
/// 指标定义非法时返回错误。重复注册视为成功。
pub fn register_pipeline_freshness_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(PIPELINE_FRESHNESS_SECONDS.clone()))
}

/// Greptime 关键表：（库, 表, 时间列）。指标表选所有服务恒有的进程指标，不依赖业务流量。
const GREPTIME_TABLES: &[(&str, &str, &str)] = &[
    (
        "mazbri_metrics",
        "process_resident_memory_bytes",
        "greptime_timestamp",
    ),
    ("mazbri_traces", "opentelemetry_traces", "timestamp"),
];

#[derive(clickhouse::Row, Deserialize)]
struct AgeRow {
    age: u64,
}

fn ch_request_logs_sql() -> String {
    format!(
        "SELECT toUInt64(least({LOOKBACK_SECS}, greatest(0, dateDiff('second', max(timestamp), now())))) AS age \
         FROM request_logs WHERE timestamp > now() - INTERVAL {LOOKBACK_SECS} SECOND"
    )
}

fn greptime_sql(db: &str, table: &str, ts: &str) -> String {
    format!(
        "SELECT now() - max({ts}) AS age FROM {db}.{table} \
         WHERE {ts} > now() - INTERVAL '{LOOKBACK_SECS} seconds'"
    )
}

/// Greptime 返回的 Duration 为纳秒整数；空窗口为 null。
fn age_from_nanos(value: Option<&serde_json::Value>) -> f64 {
    let cap = u64_to_f64(LOOKBACK_SECS);
    value
        .and_then(serde_json::Value::as_i64)
        .map_or(cap, |nanos| (i64_to_f64(nanos.max(0)) / 1e9).min(cap))
}

async fn ch_age(ch: &ClickHouseClient) -> f64 {
    match ch.execute_query::<AgeRow>(&ch_request_logs_sql()).await {
        Ok(rows) => rows
            .first()
            .map_or(u64_to_f64(LOOKBACK_SECS), |r| u64_to_f64(r.age)),
        Err(error) => {
            tracing::warn!(%error, "pipeline freshness: clickhouse query failed");
            u64_to_f64(LOOKBACK_SECS)
        }
    }
}

async fn greptime_age(client: &GreptimeDBClient, db: &str, table: &str, ts: &str) -> f64 {
    match client
        .execute_query_rows(&greptime_sql(db, table, ts), "pipeline freshness")
        .await
    {
        Ok(rows) => age_from_nanos(rows.first().and_then(|row| row.get("age"))),
        Err(error) => {
            tracing::warn!(%error, table, "pipeline freshness: greptime query failed");
            u64_to_f64(LOOKBACK_SECS)
        }
    }
}

async fn tick(greptime: Option<&GreptimeDBClient>, ch: &ClickHouseClient) {
    let age = ch_age(ch).await;
    PIPELINE_FRESHNESS_SECONDS
        .with_label_values(&["ch", "request_logs"])
        .set(age);
    if let Some(client) = greptime {
        for (db, table, ts) in GREPTIME_TABLES {
            let age = greptime_age(client, db, table, ts).await;
            PIPELINE_FRESHNESS_SECONDS
                .with_label_values(&["greptime", table])
                .set(age);
        }
    }
}

pub fn spawn(greptime: Option<Arc<GreptimeDBClient>>, ch: Arc<ClickHouseClient>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            tick(greptime.as_deref(), &ch).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nanos_are_converted_and_capped() {
        let cap = u64_to_f64(LOOKBACK_SECS);
        let two_sec = serde_json::json!(2_000_000_000_i64);
        assert!((age_from_nanos(Some(&two_sec)) - 2.0).abs() < 1e-9);
        assert!((age_from_nanos(None) - cap).abs() < 1e-9);
        let null = serde_json::Value::Null;
        assert!((age_from_nanos(Some(&null)) - cap).abs() < 1e-9);
        let huge = serde_json::json!(i64::MAX);
        assert!((age_from_nanos(Some(&huge)) - cap).abs() < 1e-9);
    }

    #[test]
    fn sql_is_bounded() {
        assert!(ch_request_logs_sql().contains("INTERVAL 86400 SECOND"));
        assert!(
            greptime_sql("mazbri_traces", "opentelemetry_traces", "timestamp")
                .contains("FROM mazbri_traces.opentelemetry_traces")
        );
    }
}
