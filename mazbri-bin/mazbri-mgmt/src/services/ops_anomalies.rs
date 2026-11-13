//! 运维异常事件与平台产品名录（压测基线 / 容量预测已随可靠性页删除）。
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

/// 时间窗 `[start, end)` 内最近的异常事件（按检测时间倒序）。
///
/// # Errors
///
/// DB 失败。
pub async fn list_anomalies(
    pool: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<AnomalyEvent>, sqlx::Error> {
    let limit = limit.clamp(1, 200);
    // 容量预测写入已删除；仍过滤历史 capacity_forecast 行，避免脏数据出现在总览。
    let rows = sqlx::query(
        "SELECT id, detected_at, kind, severity, entity_type, entity_id, metric, value, baseline, score, detail, status \
         FROM anomaly_events \
         WHERE kind != 'capacity_forecast' AND detected_at >= $1 AND detected_at < $2 \
         ORDER BY detected_at DESC LIMIT $3",
    )
    .bind(start)
    .bind(end)
    .bind(limit)
    .fetch_all(pool)
    .await?;
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

/// 平台已启用产品（id + 名称），供 `TopN` 显示名与零调用差集。
#[derive(Debug, Clone, Serialize)]
pub struct ProductName {
    pub product_id: String,
    pub name: Option<String>,
}

/// # Errors
///
/// DB 失败。
pub async fn list_platform_products(pool: &PgPool) -> Result<Vec<ProductName>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT product_id, name FROM products WHERE enabled = TRUE ORDER BY product_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ProductName {
            product_id: r.get("product_id"),
            name: r.get("name"),
        })
        .collect())
}

/// 零调用产品分页项。
#[derive(Debug, Clone, Serialize)]
pub struct ZeroCallProductItem {
    pub product_id: String,
    pub name: Option<String>,
}

/// 零调用产品服务端分页响应。
#[derive(Debug, Clone, Serialize)]
pub struct ZeroCallProductsPage {
    pub items: Vec<ZeroCallProductItem>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

#[must_use]
pub fn escape_ilike_pattern(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let escaped = trimmed
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    Some(format!("%{escaped}%"))
}

/// 平台已启用且不在 `active_product_ids` 中的零调用产品（真 SQL 分页 + 搜索）。
///
/// # Errors
///
/// DB 查询失败时返回错误。
pub async fn list_zero_call_products_page(
    pool: &PgPool,
    active_product_ids: &[String],
    page: u32,
    page_size: u32,
    q: Option<&str>,
) -> Result<ZeroCallProductsPage, sqlx::Error> {
    let page = page.max(1);
    let page_size = page_size.clamp(1, 100);
    let offset = i64::from(page - 1) * i64::from(page_size);
    let limit = i64::from(page_size);
    let like_pattern = escape_ilike_pattern(q);

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT \
         FROM products \
         WHERE enabled = TRUE \
           AND NOT (product_id = ANY($1)) \
           AND ($2::text IS NULL OR product_id ILIKE $2 ESCAPE '\\' OR COALESCE(name, '') ILIKE $2 ESCAPE '\\')",
    )
    .bind(active_product_ids)
    .bind(like_pattern.as_deref())
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query(
        "SELECT product_id, name \
         FROM products \
         WHERE enabled = TRUE \
           AND NOT (product_id = ANY($1)) \
           AND ($2::text IS NULL OR product_id ILIKE $2 ESCAPE '\\' OR COALESCE(name, '') ILIKE $2 ESCAPE '\\') \
         ORDER BY COALESCE(NULLIF(BTRIM(name), ''), product_id) ASC, product_id ASC \
         LIMIT $3 OFFSET $4",
    )
    .bind(active_product_ids)
    .bind(like_pattern.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let items = rows
        .into_iter()
        .map(|r| ZeroCallProductItem {
            product_id: r.get("product_id"),
            name: r.get("name"),
        })
        .collect();

    Ok(ZeroCallProductsPage {
        items,
        total,
        page,
        page_size,
    })
}

#[cfg(test)]
mod tests {
    use super::escape_ilike_pattern;

    #[test]
    fn escape_ilike_handles_empty_and_wildcards() {
        assert_eq!(escape_ilike_pattern(None), None);
        assert_eq!(escape_ilike_pattern(Some("   ")), None);
        assert_eq!(
            escape_ilike_pattern(Some(" 50%_test\\a ")),
            Some("%50\\%\\_test\\\\a%".to_string())
        );
    }
}
