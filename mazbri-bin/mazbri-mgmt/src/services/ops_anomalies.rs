//! 运维异常事件与零调用产品（压测基线 / 容量预测已随可靠性页删除）。
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};
use uuid::Uuid;

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

/// # Errors
///
/// DB 失败。
pub async fn list_anomalies(
    pool: &PgPool,
    limit: i64,
    kind: Option<&str>,
) -> Result<Vec<AnomalyEvent>, sqlx::Error> {
    let limit = limit.clamp(1, 200);
    let rows = if let Some(k) = kind.filter(|s| !s.is_empty()) {
        sqlx::query(
            "SELECT id, detected_at, kind, severity, entity_type, entity_id, metric, value, baseline, score, detail, status              FROM anomaly_events WHERE kind = $1 ORDER BY detected_at DESC LIMIT $2",
        )
        .bind(k)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        // 容量预测写入已删除；仍过滤历史 capacity_forecast 行，避免脏数据出现在总览。
        sqlx::query(
            "SELECT id, detected_at, kind, severity, entity_type, entity_id, metric, value, baseline, score, detail, status              FROM anomaly_events WHERE kind != 'capacity_forecast' ORDER BY detected_at DESC LIMIT $1",
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

/// 零调用产品：平台产品 − 近 30 天 CH 有调用产品（由调用方传入 CH 集合）。
#[derive(Debug, Clone, Serialize)]
pub struct ZeroCallProduct {
    pub product_id: String,
    pub name: Option<String>,
}

/// # Errors
///
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
