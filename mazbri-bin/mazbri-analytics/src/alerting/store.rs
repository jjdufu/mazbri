//! 告警规则与事件的 PostgreSQL（`mazbri_admin` 库）读写。
use super::rule::{ActiveEvent, AlertRule, CompareOp, NotifyConfig, ResolvedEvent, RuleKind};
use chrono::{DateTime, Utc};
use mazbri_core::sqlx::{self, PgPool, Row, postgres::PgRow};
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

/// 告警存储。
#[derive(Clone)]
pub struct AlertStore {
    pool: Arc<PgPool>,
}

impl AlertStore {
    #[must_use]
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// 加载全部规则；无法解析的行记录告警日志后跳过。
    ///
    /// # Errors
    ///
    /// 数据库查询失败时返回错误。
    pub async fn load_rules(&self) -> Result<Vec<AlertRule>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, name, kind, expr, threshold, op, for_seconds, severity, labels, notify, enabled \
             FROM alert_rules ORDER BY name",
        )
        .fetch_all(self.pool.as_ref())
        .await?;
        let mut rules = Vec::with_capacity(rows.len());
        for row in &rows {
            match parse_rule(row) {
                Ok(rule) => rules.push(rule),
                Err(reason) => {
                    tracing::warn!(target: "alerting", %reason, "跳过无法解析的告警规则");
                }
            }
        }
        Ok(rules)
    }

    /// 加载全部活跃（firing / acked）事件。
    ///
    /// # Errors
    ///
    /// 数据库查询失败时返回错误。
    pub async fn active_events(&self) -> Result<Vec<ActiveEvent>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, rule_id, fingerprint, value FROM alert_events \
             WHERE status IN ('firing', 'acked')",
        )
        .fetch_all(self.pool.as_ref())
        .await?;
        rows.iter()
            .map(|row| {
                Ok(ActiveEvent {
                    id: row.try_get("id")?,
                    rule_id: row.try_get("rule_id")?,
                    fingerprint: row.try_get("fingerprint")?,
                    value: row.try_get("value")?,
                })
            })
            .collect()
    }

    /// 写入 firing 事件；同指纹已有活跃事件时不写入并返回 `None`。
    ///
    /// # Errors
    ///
    /// 数据库写入失败时返回错误。
    pub async fn insert_firing(
        &self,
        rule: &AlertRule,
        fingerprint: &str,
        labels: &BTreeMap<String, String>,
        value: f64,
        started_at: DateTime<Utc>,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let labels = serde_json::to_value(labels).unwrap_or_else(|_| serde_json::json!({}));
        let row = sqlx::query(
            "INSERT INTO alert_events (rule_id, fingerprint, status, severity, labels, value, started_at) \
             VALUES ($1, $2, 'firing', $3, $4, $5, $6) \
             ON CONFLICT (fingerprint) WHERE status IN ('firing', 'acked') DO NOTHING \
             RETURNING id",
        )
        .bind(rule.id)
        .bind(fingerprint)
        .bind(&rule.severity)
        .bind(labels)
        .bind(value)
        .bind(started_at)
        .fetch_optional(self.pool.as_ref())
        .await?;
        row.map(|r| r.try_get("id")).transpose()
    }

    /// 刷新活跃事件的当前值。
    ///
    /// # Errors
    ///
    /// 数据库写入失败时返回错误。
    pub async fn refresh_value(&self, event_id: Uuid, value: f64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE alert_events SET value = $2, updated_at = now() \
             WHERE id = $1 AND status IN ('firing', 'acked')",
        )
        .bind(event_id)
        .bind(value)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    /// 将活跃事件置为 resolved；事件已不处于活跃状态时返回 `None`。
    ///
    /// # Errors
    ///
    /// 数据库写入失败时返回错误。
    pub async fn resolve(&self, event_id: Uuid) -> Result<Option<ResolvedEvent>, sqlx::Error> {
        let row = sqlx::query(
            "UPDATE alert_events SET status = 'resolved', resolved_at = now(), updated_at = now() \
             WHERE id = $1 AND status IN ('firing', 'acked') \
             RETURNING id, rule_id, labels, value, started_at, resolved_at",
        )
        .bind(event_id)
        .fetch_optional(self.pool.as_ref())
        .await?;
        row.map(|row| {
            let labels: serde_json::Value = row.try_get("labels")?;
            Ok(ResolvedEvent {
                id: row.try_get("id")?,
                rule_id: row.try_get("rule_id")?,
                labels: string_map(&labels),
                value: row.try_get("value")?,
                started_at: row.try_get("started_at")?,
                resolved_at: row
                    .try_get::<Option<DateTime<Utc>>, _>("resolved_at")?
                    .unwrap_or_else(Utc::now),
            })
        })
        .transpose()
    }
}

fn parse_rule(row: &PgRow) -> Result<AlertRule, String> {
    let get_err = |e: sqlx::Error| e.to_string();
    let id: Uuid = row.try_get("id").map_err(get_err)?;
    let kind_raw: String = row.try_get("kind").map_err(get_err)?;
    let op_raw: String = row.try_get("op").map_err(get_err)?;
    let labels: serde_json::Value = row.try_get("labels").map_err(get_err)?;
    let notify: serde_json::Value = row.try_get("notify").map_err(get_err)?;
    let for_seconds: i32 = row.try_get("for_seconds").map_err(get_err)?;
    Ok(AlertRule {
        id,
        name: row.try_get("name").map_err(get_err)?,
        kind: RuleKind::parse(&kind_raw)
            .ok_or_else(|| format!("规则 {id} kind 非法: {kind_raw}"))?,
        expr: row.try_get("expr").map_err(get_err)?,
        threshold: row.try_get("threshold").map_err(get_err)?,
        op: CompareOp::parse(&op_raw).ok_or_else(|| format!("规则 {id} op 非法: {op_raw}"))?,
        for_seconds: i64::from(for_seconds),
        severity: row.try_get("severity").map_err(get_err)?,
        labels: string_map(&labels),
        notify: serde_json::from_value::<NotifyConfig>(notify)
            .map_err(|e| format!("规则 {id} notify 非法: {e}"))?,
        enabled: row.try_get("enabled").map_err(get_err)?,
    })
}

/// 将 JSON 对象中的字符串值收集为有序标签表（非字符串值忽略）。
fn string_map(value: &serde_json::Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .map(|object| {
            object
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}
