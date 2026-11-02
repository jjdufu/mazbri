//! 告警中心：规则 CRUD、事件查询 / 确认 / 指派与概览统计（`mazbri_admin` 库）。
//!
//! 评估与通知由 analytics 的 `alerting` 模块完成，这里只负责管理面读写。
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row, postgres::PgRow};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const KINDS: [&str; 2] = ["promql", "olap"];
pub const OPS: [&str; 6] = [">", ">=", "<", "<=", "==", "!="];
pub const SEVERITIES: [&str; 3] = ["critical", "warning", "info"];
pub const STATUSES: [&str; 3] = ["firing", "acked", "resolved"];
const MAX_EXPR_LEN: usize = 4000;
const MAX_FOR_SECONDS: i32 = 86_400;
const MAX_LABELS: usize = 20;
const MAX_WEBHOOKS: usize = 5;
const MAX_EMAILS: usize = 20;
const MAX_EMAIL_LEN: usize = 254;
const MAX_PAGE_SIZE: u32 = 200;

/// 规则通知配置。`enabled=false` 时只评估落库不外发；为 true 时 emails/webhooks 至少一项。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NotifyInput {
    #[serde(default)]
    pub enabled: bool,
    pub emails: Vec<String>,
    pub webhooks: Vec<String>,
}

/// 创建 / 更新规则请求体。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertRuleInput {
    pub name: String,
    pub kind: String,
    pub expr: String,
    pub threshold: f64,
    pub op: String,
    #[serde(default)]
    pub for_seconds: i32,
    pub severity: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub notify: NotifyInput,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

const fn default_true() -> bool {
    true
}

/// 启停请求体。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnabledInput {
    pub enabled: bool,
}

/// 指派请求体；`assignee` 为空表示取消指派。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignInput {
    #[serde(default)]
    pub assignee: Option<String>,
}

/// 事件列表查询参数。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventListQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub rule_id: Option<Uuid>,
    #[serde(default)]
    pub event_id: Option<Uuid>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlertRule {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub expr: String,
    pub threshold: f64,
    pub op: String,
    pub for_seconds: i32,
    pub severity: String,
    pub labels: serde_json::Value,
    pub notify: serde_json::Value,
    pub enabled: bool,
    pub active_events: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlertEvent {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub rule_name: String,
    pub fingerprint: String,
    pub status: String,
    pub severity: String,
    pub labels: serde_json::Value,
    pub value: f64,
    pub threshold: f64,
    pub op: String,
    pub started_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub acked_by: Option<String>,
    pub acked_at: Option<DateTime<Utc>>,
    pub assignee: Option<String>,
    pub trace_ids: Vec<String>,
    pub silenced: bool,
    pub inhibited: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedEvents {
    pub data: Vec<AlertEvent>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

/// 概览「活跃告警」卡片数据。
#[derive(Debug, Serialize)]
pub struct AlertSummary {
    pub firing: i64,
    pub acked: i64,
    pub critical: i64,
    pub warning: i64,
    pub info: i64,
    pub latest: Vec<AlertEvent>,
}

/// 服务层错误。
#[derive(Debug, thiserror::Error)]
pub enum AlertError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    NotFound(&'static str),
    #[error("{0}")]
    Conflict(&'static str),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// 校验规则请求体并规范化（去除名称首尾空白）。
///
/// 不含 webhook 的 SSRF 预检（需要运行时 checker，由调用方完成）。
///
/// # Errors
///
/// 任一字段越界或格式非法时返回 [`AlertError::Validation`]。
pub fn validate_rule(mut input: AlertRuleInput) -> Result<AlertRuleInput, AlertError> {
    let bad = |m: &str| Err(AlertError::Validation(m.to_string()));
    input.name = input.name.trim().to_string();
    if input.name.is_empty() || input.name.chars().count() > 128 {
        return bad("规则名称长度必须在 1..=128 之间");
    }
    if !KINDS.contains(&input.kind.as_str()) {
        return bad("kind 只能是 promql 或 olap");
    }
    if input.expr.trim().is_empty() || input.expr.len() > MAX_EXPR_LEN {
        return bad("表达式不能为空且不超过 4000 字节");
    }
    if input.kind == "olap" {
        validate_olap_expr(&input.expr)?;
    }
    if !input.threshold.is_finite() {
        return bad("阈值必须是有限数值");
    }
    if !OPS.contains(&input.op.as_str()) {
        return bad("op 只能是 > >= < <= == !=");
    }
    if !(0..=MAX_FOR_SECONDS).contains(&input.for_seconds) {
        return bad("for_seconds 必须在 0..=86400 之间");
    }
    if !SEVERITIES.contains(&input.severity.as_str()) {
        return bad("severity 只能是 critical / warning / info");
    }
    if input.labels.len() > MAX_LABELS {
        return bad("标签最多 20 个");
    }
    for (key, value) in &input.labels {
        if !valid_label_key(key) || value.chars().count() > 256 {
            return bad("标签名须为字母或下划线开头的 1..=64 位字母数字下划线，值不超过 256 字符");
        }
    }
    for email in &mut input.notify.emails {
        *email = email.trim().to_string();
    }
    if input.notify.emails.len() > MAX_EMAILS {
        return bad("邮件收件人最多 20 个");
    }
    if !input.notify.emails.iter().all(|e| valid_email(e)) {
        return bad("邮件收件人必须是 name@domain 形式的地址且不超过 254 字节");
    }
    let mut seen = std::collections::HashSet::new();
    if !input
        .notify
        .emails
        .iter()
        .all(|e| seen.insert(e.to_ascii_lowercase()))
    {
        return bad("邮件收件人不能重复");
    }
    if input.notify.webhooks.len() > MAX_WEBHOOKS {
        return bad("webhook 最多 5 个");
    }
    for url in &input.notify.webhooks {
        if url.len() > 512 || !(url.starts_with("http://") || url.starts_with("https://")) {
            return bad("webhook 必须是 http(s) 地址且不超过 512 字节");
        }
    }
    if input.notify.enabled
        && input.notify.emails.is_empty()
        && input.notify.webhooks.is_empty()
    {
        return bad("通知已打开时必须配置至少一个邮箱或 webhook");
    }
    Ok(input)
}

/// 纯地址形式（`local@domain.tld`），不接受显示名、空白与分隔符。
fn valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    email.len() <= MAX_EMAIL_LEN
        && !local.is_empty()
        && local.len() <= 64
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && local.chars().all(|c| {
            c.is_ascii_graphic()
                && !matches!(
                    c,
                    '<' | '>' | ',' | ';' | '"' | '(' | ')' | '[' | ']' | '\\'
                )
        })
}

fn valid_label_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    key.len() <= 64
        && (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// OLAP 表达式的结构预检；模板与参数由 analytics 在评估时做完整校验。
fn validate_olap_expr(expr: &str) -> Result<(), AlertError> {
    let invalid = || {
        AlertError::Validation(
            "OLAP 表达式须为 JSON：{\"query\": {\"template\": \"...\", \"params\": {...}}, \"value_field\": \"...\"}"
                .to_string(),
        )
    };
    let value: serde_json::Value = serde_json::from_str(expr).map_err(|_| invalid())?;
    let object = value.as_object().ok_or_else(invalid)?;
    if object.keys().any(|k| k != "query" && k != "value_field") {
        return Err(invalid());
    }
    let query = object
        .get("query")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(invalid)?;
    if !query
        .get("template")
        .is_some_and(serde_json::Value::is_string)
    {
        return Err(invalid());
    }
    if object.get("value_field").is_some_and(|v| !v.is_string()) {
        return Err(invalid());
    }
    Ok(())
}

const RULE_COLUMNS: &str = "r.id, r.name, r.kind, r.expr, r.threshold, r.op, r.for_seconds, r.severity, \
     r.labels, r.notify, r.enabled, r.created_at, r.updated_at, \
     (SELECT count(*) FROM alert_events e WHERE e.rule_id = r.id AND e.status IN ('firing', 'acked')) AS active_events";

const EVENT_COLUMNS: &str = "e.id, e.rule_id, r.name AS rule_name, e.fingerprint, e.status, e.severity, e.labels, \
     e.value, r.threshold, r.op, e.started_at, e.resolved_at, e.acked_by, e.acked_at, e.assignee, \
     e.trace_ids, e.silenced, e.inhibited, e.updated_at";

fn rule_from_row(row: &PgRow) -> Result<AlertRule, sqlx::Error> {
    Ok(AlertRule {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        kind: row.try_get("kind")?,
        expr: row.try_get("expr")?,
        threshold: row.try_get("threshold")?,
        op: row.try_get("op")?,
        for_seconds: row.try_get("for_seconds")?,
        severity: row.try_get("severity")?,
        labels: row.try_get("labels")?,
        notify: row.try_get("notify")?,
        enabled: row.try_get("enabled")?,
        active_events: row.try_get("active_events")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn event_from_row(row: &PgRow) -> Result<AlertEvent, sqlx::Error> {
    Ok(AlertEvent {
        id: row.try_get("id")?,
        rule_id: row.try_get("rule_id")?,
        rule_name: row.try_get("rule_name")?,
        fingerprint: row.try_get("fingerprint")?,
        status: row.try_get("status")?,
        severity: row.try_get("severity")?,
        labels: row.try_get("labels")?,
        value: row.try_get("value")?,
        threshold: row.try_get("threshold")?,
        op: row.try_get("op")?,
        started_at: row.try_get("started_at")?,
        resolved_at: row.try_get("resolved_at")?,
        acked_by: row.try_get("acked_by")?,
        acked_at: row.try_get("acked_at")?,
        assignee: row.try_get("assignee")?,
        trace_ids: row.try_get("trace_ids")?,
        silenced: row.try_get("silenced")?,
        inhibited: row.try_get("inhibited")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn list_rules(pool: &PgPool) -> Result<Vec<AlertRule>, AlertError> {
    let rows = sqlx::query(&format!(
        "SELECT {RULE_COLUMNS} FROM alert_rules r ORDER BY r.name"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(rule_from_row).collect::<Result<_, _>>()?)
}

/// # Errors
///
/// 规则不存在或数据库失败时返回错误。
pub async fn get_rule(pool: &PgPool, id: Uuid) -> Result<AlertRule, AlertError> {
    let row = sqlx::query(&format!(
        "SELECT {RULE_COLUMNS} FROM alert_rules r WHERE r.id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AlertError::NotFound("告警规则不存在"))?;
    Ok(rule_from_row(&row)?)
}

fn map_unique(e: sqlx::Error) -> AlertError {
    match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AlertError::Conflict("同名告警规则已存在")
        }
        _ => AlertError::Db(e),
    }
}

/// # Errors
///
/// 同名冲突或数据库失败时返回错误。
pub async fn create_rule(pool: &PgPool, input: &AlertRuleInput) -> Result<AlertRule, AlertError> {
    let id: Uuid = sqlx::query(
        "INSERT INTO alert_rules (name, kind, expr, threshold, op, for_seconds, severity, labels, notify, enabled) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
    )
    .bind(&input.name)
    .bind(&input.kind)
    .bind(&input.expr)
    .bind(input.threshold)
    .bind(&input.op)
    .bind(input.for_seconds)
    .bind(&input.severity)
    .bind(sqlx::types::Json(&input.labels))
    .bind(sqlx::types::Json(&input.notify))
    .bind(input.enabled)
    .fetch_one(pool)
    .await
    .map_err(map_unique)?
    .try_get("id")?;
    get_rule(pool, id).await
}

/// # Errors
///
/// 规则不存在、同名冲突或数据库失败时返回错误。
pub async fn update_rule(
    pool: &PgPool,
    id: Uuid,
    input: &AlertRuleInput,
) -> Result<AlertRule, AlertError> {
    let result = sqlx::query(
        "UPDATE alert_rules SET name = $2, kind = $3, expr = $4, threshold = $5, op = $6, \
         for_seconds = $7, severity = $8, labels = $9, notify = $10, enabled = $11, updated_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(&input.name)
    .bind(&input.kind)
    .bind(&input.expr)
    .bind(input.threshold)
    .bind(&input.op)
    .bind(input.for_seconds)
    .bind(&input.severity)
    .bind(sqlx::types::Json(&input.labels))
    .bind(sqlx::types::Json(&input.notify))
    .bind(input.enabled)
    .execute(pool)
    .await
    .map_err(map_unique)?;
    if result.rows_affected() == 0 {
        return Err(AlertError::NotFound("告警规则不存在"));
    }
    get_rule(pool, id).await
}

/// # Errors
///
/// 规则不存在或数据库失败时返回错误。
pub async fn set_rule_enabled(
    pool: &PgPool,
    id: Uuid,
    enabled: bool,
) -> Result<AlertRule, AlertError> {
    let result =
        sqlx::query("UPDATE alert_rules SET enabled = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(enabled)
            .execute(pool)
            .await?;
    if result.rows_affected() == 0 {
        return Err(AlertError::NotFound("告警规则不存在"));
    }
    get_rule(pool, id).await
}

/// 删除规则（其事件随外键级联删除）。
///
/// # Errors
///
/// 规则不存在或数据库失败时返回错误。
pub async fn delete_rule(pool: &PgPool, id: Uuid) -> Result<AlertRule, AlertError> {
    let rule = get_rule(pool, id).await?;
    sqlx::query("DELETE FROM alert_rules WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(rule)
}

/// # Errors
///
/// 过滤参数非法或数据库失败时返回错误。
pub async fn list_events(
    pool: &PgPool,
    query: &EventListQuery,
) -> Result<PaginatedEvents, AlertError> {
    if let Some(status) = query.status.as_deref()
        && !STATUSES.contains(&status)
        && status != "active"
    {
        return Err(AlertError::Validation(
            "status 只能是 firing / acked / resolved / active".to_string(),
        ));
    }
    if let Some(severity) = query.severity.as_deref()
        && !SEVERITIES.contains(&severity)
    {
        return Err(AlertError::Validation(
            "severity 只能是 critical / warning / info".to_string(),
        ));
    }
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, MAX_PAGE_SIZE);
    let filter = "($1::text IS NULL OR e.status = $1 OR ($1 = 'active' AND e.status IN ('firing', 'acked'))) \
         AND ($2::text IS NULL OR e.severity = $2) \
         AND ($3::uuid IS NULL OR e.rule_id = $3) \
         AND ($4::uuid IS NULL OR e.id = $4)";
    let total: i64 = sqlx::query(&format!(
        "SELECT count(*) AS total FROM alert_events e WHERE {filter}"
    ))
    .bind(query.status.as_deref())
    .bind(query.severity.as_deref())
    .bind(query.rule_id)
    .bind(query.event_id)
    .fetch_one(pool)
    .await?
    .try_get("total")?;
    let rows = sqlx::query(&format!(
        "SELECT {EVENT_COLUMNS} FROM alert_events e JOIN alert_rules r ON r.id = e.rule_id \
         WHERE {filter} ORDER BY e.started_at DESC, e.id LIMIT $5 OFFSET $6"
    ))
    .bind(query.status.as_deref())
    .bind(query.severity.as_deref())
    .bind(query.rule_id)
    .bind(query.event_id)
    .bind(i64::from(page_size))
    .bind(i64::from(page - 1) * i64::from(page_size))
    .fetch_all(pool)
    .await?;
    Ok(PaginatedEvents {
        data: rows.iter().map(event_from_row).collect::<Result<_, _>>()?,
        total,
        page,
        page_size,
    })
}

async fn get_event(pool: &PgPool, id: Uuid) -> Result<AlertEvent, AlertError> {
    let row = sqlx::query(&format!(
        "SELECT {EVENT_COLUMNS} FROM alert_events e JOIN alert_rules r ON r.id = e.rule_id WHERE e.id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AlertError::NotFound("告警事件不存在"))?;
    Ok(event_from_row(&row)?)
}

/// 确认 firing 事件（事件仍保持活跃，恢复后由评估器置为 resolved）。
///
/// # Errors
///
/// 事件不存在、不处于 firing 状态或数据库失败时返回错误。
pub async fn ack_event(pool: &PgPool, id: Uuid, username: &str) -> Result<AlertEvent, AlertError> {
    let result = sqlx::query(
        "UPDATE alert_events SET status = 'acked', acked_by = $2, acked_at = now(), updated_at = now() \
         WHERE id = $1 AND status = 'firing'",
    )
    .bind(id)
    .bind(username)
    .execute(pool)
    .await?;
    let event = get_event(pool, id).await?;
    if result.rows_affected() == 0 {
        return Err(AlertError::Conflict("只有触发中的告警可以确认"));
    }
    Ok(event)
}

/// 指派处理人；处理人必须是启用中的管理员账号。
///
/// # Errors
///
/// 处理人不存在、事件不存在或数据库失败时返回错误。
pub async fn assign_event(
    pool: &PgPool,
    id: Uuid,
    assignee: Option<&str>,
) -> Result<AlertEvent, AlertError> {
    let assignee = assignee.map(str::trim).filter(|s| !s.is_empty());
    if let Some(name) = assignee {
        let exists = sqlx::query("SELECT 1 FROM admin_users WHERE username = $1 AND is_active")
            .bind(name)
            .fetch_optional(pool)
            .await?
            .is_some();
        if !exists {
            return Err(AlertError::Validation(
                "指派对象不是启用中的管理员".to_string(),
            ));
        }
    }
    let result =
        sqlx::query("UPDATE alert_events SET assignee = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(assignee)
            .execute(pool)
            .await?;
    if result.rows_affected() == 0 {
        return Err(AlertError::NotFound("告警事件不存在"));
    }
    get_event(pool, id).await
}

/// # Errors
///
/// 数据库失败时返回错误。
pub async fn summary(pool: &PgPool) -> Result<AlertSummary, AlertError> {
    let row = sqlx::query(
        "SELECT count(*) FILTER (WHERE status = 'firing') AS firing, \
                count(*) FILTER (WHERE status = 'acked') AS acked, \
                count(*) FILTER (WHERE severity = 'critical') AS critical, \
                count(*) FILTER (WHERE severity = 'warning') AS warning, \
                count(*) FILTER (WHERE severity = 'info') AS info \
         FROM alert_events WHERE status IN ('firing', 'acked')",
    )
    .fetch_one(pool)
    .await?;
    let latest = list_events(
        pool,
        &EventListQuery {
            status: Some("active".to_string()),
            page_size: Some(5),
            ..EventListQuery::default()
        },
    )
    .await?
    .data;
    Ok(AlertSummary {
        firing: row.try_get("firing")?,
        acked: row.try_get("acked")?,
        critical: row.try_get("critical")?,
        warning: row.try_get("warning")?,
        info: row.try_get("info")?,
        latest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> AlertRuleInput {
        AlertRuleInput {
            name: "  熔断打开 ".to_string(),
            kind: "promql".to_string(),
            expr: "max by (name) (circuit_breaker_state)".to_string(),
            threshold: 2.0,
            op: ">=".to_string(),
            for_seconds: 0,
            severity: "critical".to_string(),
            labels: BTreeMap::from([("category".to_string(), "upstream".to_string())]),
            notify: NotifyInput::default(),
            enabled: true,
        }
    }

    #[test]
    fn valid_rule_is_normalized() {
        let rule = validate_rule(input()).expect("valid");
        assert_eq!(rule.name, "熔断打开");
    }

    #[test]
    fn invalid_fields_are_rejected() {
        let cases: Vec<fn(&mut AlertRuleInput)> = vec![
            |r| r.name = "   ".to_string(),
            |r| r.kind = "sql".to_string(),
            |r| r.expr = String::new(),
            |r| r.threshold = f64::NAN,
            |r| r.op = "=>".to_string(),
            |r| r.for_seconds = -1,
            |r| r.for_seconds = 86_401,
            |r| r.severity = "fatal".to_string(),
            |r| {
                r.labels.insert("1bad".to_string(), "x".to_string());
            },
            |r| r.notify.webhooks = vec!["ftp://example.com".to_string()],
            |r| r.notify.emails = vec!["no-at-sign".to_string()],
            |r| r.notify.emails = vec!["Ops <ops@example.com>".to_string()],
            |r| r.notify.emails = vec!["a@b@example.com".to_string()],
            |r| r.notify.emails = vec!["ops@localhost".to_string()],
            |r| {
                r.notify.emails =
                    vec!["ops@example.com".to_string(), "OPS@example.com".to_string()];
            },
            |r| r.notify.emails = (0..21).map(|i| format!("u{i}@example.com")).collect(),
            |r| r.notify.webhooks = vec!["https://example.com".to_string(); 6],
        ];
        for mutate in cases {
            let mut rule = input();
            mutate(&mut rule);
            assert!(validate_rule(rule).is_err());
        }
    }

    #[test]
    fn email_recipients_are_trimmed_and_accepted() {
        let mut rule = input();
        rule.notify.emails = vec![
            " ops@example.com ".to_string(),
            "alerts+db@corp.example.cn".to_string(),
        ];
        let rule = validate_rule(rule).expect("valid");
        assert_eq!(rule.notify.emails[0], "ops@example.com");
    }

    #[test]
    fn notify_requires_both_channel_fields() {
        let raw = serde_json::json!({
            "name": "x", "kind": "promql", "expr": "up", "threshold": 1, "op": ">",
            "severity": "info", "notify": {"webhooks": []}
        });
        assert!(serde_json::from_value::<AlertRuleInput>(raw).is_err());
    }

    #[test]
    fn olap_expr_shape_is_checked() {
        let mut rule = input();
        rule.kind = "olap".to_string();
        rule.expr = r#"{"query":{"template":"top_by_dim","params":{"dim":"product_id"}},"value_field":"value"}"#.to_string();
        assert!(validate_rule(rule.clone()).is_ok());
        for bad in [
            "not json",
            "[]",
            r#"{"query":{}}"#,
            r#"{"query":{"template":"x"},"extra":1}"#,
            r#"{"query":{"template":"x"},"value_field":3}"#,
        ] {
            rule.expr = bad.to_string();
            assert!(validate_rule(rule.clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn unknown_request_fields_are_rejected() {
        let raw = serde_json::json!({
            "name": "x", "kind": "promql", "expr": "up", "threshold": 1, "op": ">",
            "severity": "info", "notify": {"slack": "x"}
        });
        assert!(serde_json::from_value::<AlertRuleInput>(raw).is_err());
        let raw = serde_json::json!({"status": "firing", "q": "x"});
        assert!(serde_json::from_value::<EventListQuery>(raw).is_err());
    }
}
