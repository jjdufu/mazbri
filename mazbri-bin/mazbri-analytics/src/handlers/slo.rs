//! 步骤 24：SLO 状态（读 Greptime `slo_sli_5m`）。

use super::health::AppState;
use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;

/// `GET /api/v1/slo/status`
pub async fn get_slo_status(State(state): State<AppState>) -> axum::response::Response {
    let Some(client) = state.greptimedb_client.as_ref() else {
        return Json(json!({"items": [], "error": "greptime_unavailable"})).into_response();
    };
    let sql = r"
SELECT slo, sli, error_budget_remaining, burn_1h, burn_5m, burn_page, burn_ticket, ts
FROM slo_sli_5m
ORDER BY ts DESC
LIMIT 50
";
    match client.execute_query_rows(sql, "slo status").await {
        Ok(rows) => {
            // 按 slo 去重取最新
            let mut seen = std::collections::BTreeSet::new();
            let mut items = Vec::new();
            for row in rows {
                let name = row
                    .get("slo")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() || !seen.insert(name.clone()) {
                    continue;
                }
                items.push(json!({
                    "name": name,
                    "sli": row.get("sli"),
                    "error_budget_remaining": row.get("error_budget_remaining"),
                    "burn_1h": row.get("burn_1h"),
                    "burn_5m": row.get("burn_5m"),
                    "burn_page": row.get("burn_page"),
                    "burn_ticket": row.get("burn_ticket"),
                    "ts": row.get("ts"),
                }));
            }
            Json(json!({"items": items})).into_response()
        }
        Err(e) => {
            tracing::warn!(target: "slo", error = %e, "查询 slo_sli_5m 失败");
            Json(json!({"items": [], "error": e.to_string()})).into_response()
        }
    }
}
