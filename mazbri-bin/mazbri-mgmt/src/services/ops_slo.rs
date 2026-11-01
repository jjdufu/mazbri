//! 步骤 24：SLO 定义查询与状态（读 Greptime `slo_sli_5m`）。

use mazbri_core::sqlx::{self, PgPool, Row};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Serialize)]
pub struct SloDefinition {
    pub id: String,
    pub name: String,
    pub sli_type: String,
    pub query_kind: String,
    pub query: String,
    pub objective: f64,
    pub window_days: i32,
    pub scope: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct SloStatus {
    pub name: String,
    pub sli: Option<f64>,
    pub error_budget_remaining: Option<f64>,
    pub burn_page: Option<f64>,
    pub burn_ticket: Option<f64>,
    pub burn_1h: Option<f64>,
    pub burn_5m: Option<f64>,
    pub ts: Option<String>,
}

/// # Errors
/// DB 失败。
pub async fn list_definitions(pool: &PgPool) -> Result<serde_json::Value, String> {
    let rows = sqlx::query(
        "SELECT id, name, sli_type, query_kind, query, objective, window_days, scope, enabled \
         FROM slo_definitions ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(SloDefinition {
            id: row
                .try_get::<uuid::Uuid, _>("id")
                .map_err(|e| e.to_string())?
                .to_string(),
            name: row.try_get("name").map_err(|e| e.to_string())?,
            sli_type: row.try_get("sli_type").map_err(|e| e.to_string())?,
            query_kind: row.try_get("query_kind").map_err(|e| e.to_string())?,
            query: row.try_get("query").map_err(|e| e.to_string())?,
            objective: row.try_get("objective").map_err(|e| e.to_string())?,
            window_days: row.try_get("window_days").map_err(|e| e.to_string())?,
            scope: row.try_get("scope").map_err(|e| e.to_string())?,
            enabled: row.try_get("enabled").map_err(|e| e.to_string())?,
        });
    }
    Ok(json!({ "items": items }))
}

/// # Errors
/// DB 失败。
pub async fn list_status(pool: &PgPool) -> Result<serde_json::Value, String> {
    // 状态以 PG 侧镜像表不可用时，退回定义列表 + 空指标；
    // 实际时序在 Greptime，由 analytics 代理或前端直接读 overview。
    // 这里用最近一次写入到 alert 无关的方式：读定义，状态字段留给 analytics HTTP。
    let defs = list_definitions(pool).await?;
    Ok(json!({
        "definitions": defs.get("items").cloned().unwrap_or(json!([])),
        "note": "时序见 Greptime slo_sli_5m / 指标 mazbri_slo_*；本接口返回定义清单"
    }))
}
