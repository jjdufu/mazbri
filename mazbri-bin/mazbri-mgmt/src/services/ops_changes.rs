//! 步骤 22 / 步骤 14：变更与发布时间线聚合（精准收敛真实线上写变更锚点）。

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{self, PgPool, Row};

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

const EXCLUDED_EXACT_ACTIONS: &[&str] = &[
    "admin.login",
    "admin.logout",
    "authorization.root_material_read",
    "authorization.license_request",
    "supplier.validate_config",
    "tenant.crypto_verify",
    "connector.run",
    "connector.transform_preview",
    "config_center.validate",
    "upstream.onboarding_dry_run",
    "certificate.verify",
    "api.read",
    "api.get",
];

const OPS_CHANGES_SQL: &str = r"
SELECT created_at, action, actor_username, resource_type, resource_id,
       status, service, failure_reason, details, old_value, new_value
FROM audit_logs_view
WHERE created_at >= $1 AND created_at < $2
  AND status = 'success'
  AND COALESCE(path, '') NOT LIKE '%/admin/preferences'
  AND action NOT LIKE 'audit.%'
  AND action NOT LIKE 'user.%'
  AND action NOT LIKE 'password.%'
  AND action NOT IN (
      'admin.login',
      'admin.logout',
      'authorization.root_material_read',
      'authorization.license_request',
      'supplier.validate_config',
      'tenant.crypto_verify',
      'connector.run',
      'connector.transform_preview',
      'config_center.validate',
      'upstream.onboarding_dry_run',
      'certificate.verify',
      'api.read',
      'api.get'
  )
  AND action NOT LIKE '%.list'
  AND action NOT LIKE '%.view%'
  AND action NOT LIKE '%.export'
  AND action NOT LIKE '%.draft\_%' ESCAPE '\'
ORDER BY created_at DESC
LIMIT $3
";

/// 判断审计记录是否属于运行指标时间线的真实线上写变更事件。
#[must_use]
pub fn is_ops_change_event(action: &str, status: &str) -> bool {
    if status != "success" {
        return false;
    }
    if action.starts_with("audit.")
        || action.starts_with("user.")
        || action.starts_with("password.")
    {
        return false;
    }
    if EXCLUDED_EXACT_ACTIONS.contains(&action) {
        return false;
    }
    if let Some((_, verb)) = action.split_once('.')
        && (verb == "list"
            || verb == "export"
            || verb.starts_with("view")
            || verb.starts_with("draft_"))
    {
        return false;
    }
    true
}

/// 聚合最近真实写变更：仅保留成功的配置/路由/发布/证书等线上变更事件，按时间倒序。
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
    let rows = sqlx::query(OPS_CHANGES_SQL)
        .bind(start)
        .bind(end)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    let mut events = Vec::with_capacity(rows.len());
    for row in rows {
        let action: String = row.get("action");
        let status: String = row.get("status");
        if !is_ops_change_event(&action, &status) {
            continue;
        }
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
    use super::{OPS_CHANGES_SQL, classify_action, is_ops_change_event};

    #[test]
    fn classify_maps_common_actions() {
        assert_eq!(classify_action("route_update", Some("route")), "route");
        assert_eq!(classify_action("connector_publish", None), "publish");
        assert_eq!(classify_action("license_import", None), "license");
        assert_eq!(classify_action("appearance_update", None), "config");
        assert_eq!(classify_action("login_success", None), "auth");
        assert_eq!(classify_action("product_create", Some("product")), "audit");
    }

    #[test]
    fn ops_changes_sql_and_predicate_filter_noise_and_keep_real_mutations() {
        assert!(OPS_CHANGES_SQL.contains("status = 'success'"));
        assert!(OPS_CHANGES_SQL.contains("action NOT LIKE 'audit.%'"));
        assert!(!OPS_CHANGES_SQL.contains("audit_log.%"));

        for excluded in [
            "audit.list",
            "audit.view",
            "audit.export",
            "authorization.root_material_read",
            "authorization.license_request",
            "supplier.validate_config",
            "tenant.crypto_verify",
            "connector.run",
            "connector.draft_create",
            "connector.draft_validate",
            "certificate.view_full",
            "apikey.list",
            "config_center.export",
            "user.login",
            "user.mfa_step_up",
            "password.change",
        ] {
            assert!(
                !is_ops_change_event(excluded, "success"),
                "{excluded} should be excluded from ops changes"
            );
        }

        assert!(
            !is_ops_change_event("route.emergency_switch", "failure"),
            "failed mutations should not pollute ops change anchors"
        );

        for retained in [
            "route.emergency_switch",
            "connector.force_publish",
            "connector.publish",
            "upstream.onboarding",
            "certificate.renew",
            "tenant.ratelimit_update",
            "config_center.update",
            "authorization.import",
        ] {
            assert!(
                is_ops_change_event(retained, "success"),
                "{retained} should be retained as real ops change"
            );
        }
    }
}
