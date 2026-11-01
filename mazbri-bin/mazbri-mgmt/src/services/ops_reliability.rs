//! 步骤 26：压测基线、异常事件、零调用产品。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{self, PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct LoadtestRun {
    pub id: Uuid,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub scenario: String,
    pub products: i32,
    pub requests: i64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub internal_p99: f64,
    pub notes: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateLoadtestRun {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub scenario: String,
    pub products: i32,
    pub requests: i64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub internal_p99: f64,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnomalyEvent {
    pub id: Uuid,
    pub detected_at: DateTime<Utc>,
    pub kind: String,
    pub severity: String,
    pub entity_type: String,
    pub entity_id: String,
    pub metric: String,
    pub value: f64,
    pub baseline: f64,
    pub score: f64,
    pub detail: serde_json::Value,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapacitySnapshot {
    pub detected_at: Option<DateTime<Utc>>,
    pub holt_next: Option<f64>,
    pub qps_cap: Option<f64>,
    pub days_to_cap: Option<f64>,
    pub detail: serde_json::Value,
}

/// # Errors
/// DB 失败。
pub async fn list_loadtests(pool: &PgPool, limit: i64) -> Result<Vec<LoadtestRun>, sqlx::Error> {
    let limit = limit.clamp(1, 50);
    let rows = sqlx::query(
        "SELECT id, started_at, ended_at, scenario, products, requests, p50, p95, p99, internal_p99, notes \
         FROM loadtest_runs ORDER BY started_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| LoadtestRun {
            id: row.get("id"),
            started_at: row.get("started_at"),
            ended_at: row.get("ended_at"),
            scenario: row.get("scenario"),
            products: row.get("products"),
            requests: row.get("requests"),
            p50: row.get("p50"),
            p95: row.get("p95"),
            p99: row.get("p99"),
            internal_p99: row.get("internal_p99"),
            notes: row.get("notes"),
        })
        .collect())
}

/// # Errors
/// DB 失败。
pub async fn create_loadtest(
    pool: &PgPool,
    req: &CreateLoadtestRun,
) -> Result<LoadtestRun, sqlx::Error> {
    let row = sqlx::query(
        "INSERT INTO loadtest_runs (started_at, ended_at, scenario, products, requests, p50, p95, p99, internal_p99, notes) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) \
         RETURNING id, started_at, ended_at, scenario, products, requests, p50, p95, p99, internal_p99, notes",
    )
    .bind(req.started_at)
    .bind(req.ended_at)
    .bind(&req.scenario)
    .bind(req.products)
    .bind(req.requests)
    .bind(req.p50)
    .bind(req.p95)
    .bind(req.p99)
    .bind(req.internal_p99)
    .bind(&req.notes)
    .fetch_one(pool)
    .await?;
    Ok(LoadtestRun {
        id: row.get("id"),
        started_at: row.get("started_at"),
        ended_at: row.get("ended_at"),
        scenario: row.get("scenario"),
        products: row.get("products"),
        requests: row.get("requests"),
        p50: row.get("p50"),
        p95: row.get("p95"),
        p99: row.get("p99"),
        internal_p99: row.get("internal_p99"),
        notes: row.get("notes"),
    })
}

/// # Errors
/// DB 失败。
pub async fn list_anomalies(
    pool: &PgPool,
    limit: i64,
    kind: Option<&str>,
) -> Result<Vec<AnomalyEvent>, sqlx::Error> {
    let limit = limit.clamp(1, 200);
    let rows = if let Some(k) = kind.filter(|s| !s.is_empty()) {
        sqlx::query(
            "SELECT id, detected_at, kind, severity, entity_type, entity_id, metric, value, baseline, score, detail, status \
             FROM anomaly_events WHERE kind = $1 ORDER BY detected_at DESC LIMIT $2",
        )
        .bind(k)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT id, detected_at, kind, severity, entity_type, entity_id, metric, value, baseline, score, detail, status \
             FROM anomaly_events WHERE kind != 'capacity_forecast' ORDER BY detected_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?
    };
    Ok(rows
        .into_iter()
        .map(|row| AnomalyEvent {
            id: row.get("id"),
            detected_at: row.get("detected_at"),
            kind: row.get("kind"),
            severity: row.get("severity"),
            entity_type: row.get("entity_type"),
            entity_id: row.get("entity_id"),
            metric: row.get("metric"),
            value: row.get("value"),
            baseline: row.get("baseline"),
            score: row.get("score"),
            detail: row.get("detail"),
            status: row.get("status"),
        })
        .collect())
}

/// # Errors
/// DB 失败。
pub async fn latest_capacity(pool: &PgPool) -> Result<CapacitySnapshot, sqlx::Error> {
    let row = sqlx::query(
        "SELECT detected_at, value, baseline, score, detail FROM anomaly_events \
         WHERE kind = 'capacity_forecast' ORDER BY detected_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some(r) => {
            let detail: serde_json::Value = r.get("detail");
            CapacitySnapshot {
                detected_at: Some(r.get("detected_at")),
                holt_next: Some(r.get("value")),
                qps_cap: Some(r.get("baseline")),
                days_to_cap: {
                    let s: f64 = r.get("score");
                    if s < 0.0 {
                        None
                    } else {
                        Some(s)
                    }
                },
                detail,
            }
        }
        None => CapacitySnapshot {
            detected_at: None,
            holt_next: None,
            qps_cap: None,
            days_to_cap: None,
            detail: serde_json::json!({}),
        },
    })
}

/// 零调用产品：平台产品 − 近 30 天 CH 有调用产品（由调用方传入 CH 集合）。
#[derive(Debug, Clone, Serialize)]
pub struct ZeroCallProduct {
    pub product_id: String,
    pub name: Option<String>,
}

/// # Errors
/// DB 失败。
pub async fn list_platform_products(
    pool: &PgPool,
) -> Result<Vec<(String, Option<String>)>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT product_id, name FROM products WHERE enabled = TRUE ORDER BY product_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get("product_id"), r.get("name")))
        .collect())
}
