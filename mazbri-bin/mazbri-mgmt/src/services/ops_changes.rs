//! 步骤 22：变更与发布时间线聚合（审计 + 路由/发布/版本指标标记）。

use chrono::{DateTime, Utc};
use sqlx::{self, PgPool, Row};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ChangeEvent {
    pub ts: DateTime<Utc>,
    pub kind: String,
    pub title: String,
    pub actor: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub status: Option<String>,
    pub service: Option<String>,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangesResponse {
    pub events: Vec<ChangeEvent>,
}

/// 聚合最近变更：管理审计成功/失败写入，按时间倒序。
///
/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn list_changes(
    pool: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    limit: i64,
) -> Result<ChangesResponse, sqlx::Error> {
    let limit = limit.clamp(1, 200);
    let rows = sqlx::query(
        r"
        SELECT created_at, action, actor_username, resource_type, resource_id,
               status, service, failure_reason, details, old_value, new_value
        FROM audit_logs_view
        WHERE created_at >= $1 AND created_at < $2
        ORDER BY created_at DESC
        LIMIT $3
        ",
    )
    .bind(start)
    .bind(end)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut events = Vec::with_capacity(rows.len());
    for row in rows {
        let action: String = row.get("action");
        let status: String = row.get("status");
        let actor: Option<String> = row.get("actor_username");
        let resource_type: Option<String> = row.get("resource_type");
        let resource_id: Option<String> = row.get("resource_id");
        let service: Option<String> = row.get("service");
        let failure_reason: Option<String> = row.get("failure_reason");
        let details: Option<serde_json::Value> = row.get("details");
        let old_value: Option<serde_json::Value> = row.get("old_value");
        let new_value: Option<serde_json::Value> = row.get("new_value");
        let created_at: DateTime<Utc> = row.get("created_at");
        let kind = classify_action(&action, resource_type.as_deref());
        let title = format!("{action} ({status})");
        events.push(ChangeEvent {
            ts: created_at,
            kind,
            title,
            actor,
            resource_type,
            resource_id,
            status: Some(status),
            service,
            detail: serde_json::json!({
                "failure_reason": failure_reason,
                "details": details,
                "old_value": old_value,
                "new_value": new_value,
            }),
        });
    }
    Ok(ChangesResponse { events })
}

fn classify_action(action: &str, resource_type: Option<&str>) -> String {
    let a = action.to_ascii_lowercase();
    if a.contains("route") || resource_type == Some("route") {
        return "route".into();
    }
    if a.contains("publish") || a.contains("connector") {
        return "publish".into();
    }
    if a.contains("license") || a.contains("cert") {
        return "license".into();
    }
    if a.contains("config") || a.contains("setting") || a.contains("appearance") {
        return "config".into();
    }
    if a.contains("login") || a.contains("mfa") {
        return "auth".into();
    }
    "audit".into()
}

#[cfg(test)]
mod tests {
    use super::classify_action;

    #[test]
    fn classify_maps_common_actions() {
        assert_eq!(classify_action("route_update", Some("route")), "route");
        assert_eq!(classify_action("connector_publish", None), "publish");
        assert_eq!(classify_action("license_import", None), "license");
        assert_eq!(classify_action("appearance_update", None), "config");
        assert_eq!(classify_action("login_success", None), "auth");
        assert_eq!(classify_action("product_create", Some("product")), "audit");
    }
}
