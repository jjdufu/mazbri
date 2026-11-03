//! 最小告警中心：周期评估 `alert_rules`，按持续时间与指纹去重写入 `alert_events`，
//! 触发 / 恢复时按规则配置发送邮件（默认渠道）与 webhook（可选）通知；
//! 多实例部署时每轮评估由 PG advisory lock 互斥，只有持锁实例执行。
pub mod engine;
pub mod filter;
pub mod notifier;
pub mod rule;
pub mod store;

use crate::clickhouse::ClickHouseClient;
use crate::clickhouse::olap::{ExemplarMetric, ExemplarParams, Filters, OlapQuery, OlapRows};
use crate::greptimedb::GreptimeDBClient;
use chrono::Utc;
use engine::{Action, Engine};
use filter::{is_inhibited, is_silenced, InhibitRule};
use notifier::{Notifier, SmtpConfig};
use rule::{
    ActiveEvent, AlertNotification, AlertRule, OlapExpr, RuleKind, Sample,
    samples_from_rows,
};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use store::{AlertStore, EvaluationSession, FireInsert};
use uuid::Uuid;

/// `[alerting]` 配置段。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertingConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// 评估周期（秒）。
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// 单次 webhook 请求超时（秒）。
    #[serde(default = "default_webhook_timeout")]
    pub webhook_timeout_secs: u64,
    /// 控制台对外访问地址，用于通知中的事件链接。
    #[serde(default)]
    pub console_base_url: Option<String>,
    /// 告警表所在的 `mazbri_admin` 库连接。
    pub database: mazbri_core::DatabaseConfig,
    /// 邮件通知的 SMTP 服务器；未配置时规则中的邮件收件人不会收到通知。
    #[serde(default)]
    pub smtp: Option<SmtpConfig>,
}

const fn default_enabled() -> bool {
    true
}
const fn default_interval() -> u64 {
    30
}
const fn default_webhook_timeout() -> u64 {
    5
}

impl AlertingConfig {
    /// # Errors
    ///
    /// 周期或超时超出允许范围、控制台地址非 http(s) 时返回错误。
    pub fn validate(&self) -> Result<(), String> {
        if !(5..=3600).contains(&self.interval_secs) {
            return Err("alerting.interval_secs 必须在 5..=3600 之间".to_string());
        }
        if !(1..=60).contains(&self.webhook_timeout_secs) {
            return Err("alerting.webhook_timeout_secs 必须在 1..=60 之间".to_string());
        }
        if let Some(url) = &self.console_base_url
            && !url.starts_with("http://")
            && !url.starts_with("https://")
        {
            return Err("alerting.console_base_url 必须以 http:// 或 https:// 开头".to_string());
        }
        if let Some(smtp) = &self.smtp {
            smtp.validate()?;
        }
        Ok(())
    }
}

/// 告警评估器。
pub struct Evaluator {
    store: AlertStore,
    greptimedb: Option<Arc<GreptimeDBClient>>,
    clickhouse: Arc<ClickHouseClient>,
    notifier: Notifier,
    engine: Engine,
}

impl Evaluator {
    #[must_use]
    pub fn new(
        store: AlertStore,
        greptimedb: Option<Arc<GreptimeDBClient>>,
        clickhouse: Arc<ClickHouseClient>,
        notifier: Notifier,
    ) -> Self {
        Self {
            store,
            greptimedb,
            clickhouse,
            notifier,
            engine: Engine::default(),
        }
    }

    /// 后台循环：每 `interval` 评估一轮，直到进程退出。
    pub async fn run(mut self, interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            if let Err(error) = self.tick().await {
                tracing::warn!(target: "alerting", %error, "告警评估轮次失败");
            }
        }
    }

    /// 执行一轮评估。
    ///
    /// # Errors
    ///
    /// 规则或活跃事件加载失败时返回错误；单条规则查询或写入失败只记日志。
    pub async fn tick(&mut self) -> Result<(), mazbri_core::sqlx::Error> {
        let Some(mut session) = self.store.try_begin_evaluation().await? else {
            // 其他实例持锁评估本轮；清空本地持续计时，接手时从头计算。
            self.engine.retain_rules(&HashSet::new());
            tracing::debug!(target: "alerting", "其他实例正在评估，本轮跳过");
            return Ok(());
        };
        let result = self.evaluate(&mut session).await;
        session.release().await;
        result
    }

    async fn evaluate(
        &mut self,
        session: &mut EvaluationSession,
    ) -> Result<(), mazbri_core::sqlx::Error> {
        let rules = session.load_rules().await?;
        let silences = session.load_silences().await?;
        let inhibit_rules = session.load_inhibit_rules().await?;
        let mut active_by_rule: HashMap<Uuid, HashMap<String, ActiveEvent>> = HashMap::new();
        for event in session.active_events().await? {
            active_by_rule
                .entry(event.rule_id)
                .or_default()
                .insert(event.fingerprint.clone(), event);
        }
        let enabled: HashSet<Uuid> = rules.iter().filter(|r| r.enabled).map(|r| r.id).collect();
        self.engine.retain_rules(&enabled);
        let rules_by_id: HashMap<Uuid, &AlertRule> = rules.iter().map(|r| (r.id, r)).collect();
        let empty = HashMap::new();
        let now = Utc::now();
        let mut pending_fires: Vec<PendingFire> = Vec::new();
        for rule in rules.iter().filter(|r| r.enabled) {
            let samples = match self.samples(rule).await {
                Ok(samples) => samples,
                Err(error) => {
                    tracing::warn!(target: "alerting", rule = %rule.name, %error, "告警规则查询失败，本轮跳过");
                    continue;
                }
            };
            let active = active_by_rule.get(&rule.id).unwrap_or(&empty);
            let actions = self.engine.plan_rule(rule, &samples, active, now);
            for action in actions {
                match action {
                    Action::Fire {
                        fingerprint,
                        labels,
                        value,
                        started_at,
                        ..
                    } => {
                        let muted = is_silenced(&silences, &labels, now);
                        let traces = self.exemplar_trace_ids(rule, &labels).await;
                        match session
                            .insert_firing(FireInsert {
                                rule,
                                fingerprint: &fingerprint,
                                labels: &labels,
                                value,
                                started_at,
                                trace_ids: &traces,
                                silenced: muted,
                                inhibited: false,
                            })
                            .await
                        {
                            Ok(Some(event_id)) => {
                                tracing::warn!(
                                    target: "alerting",
                                    rule = %rule.name,
                                    %event_id,
                                    value,
                                    muted,
                                    "告警触发"
                                );
                                pending_fires.push(PendingFire {
                                    event_id,
                                    rule_id: rule.id,
                                    labels,
                                    value,
                                    started_at,
                                    silenced: muted,
                                });
                            }
                            Ok(None) => {}
                            Err(error) => {
                                tracing::warn!(target: "alerting", rule = %rule.name, %error, "写入告警事件失败");
                            }
                        }
                    }
                    other => self.apply_non_fire(session, rule, other).await,
                }
            }
        }
        self.notify_pending_fires(session, &rules_by_id, &inhibit_rules, pending_fires)
            .await;
        self.resolve_disabled(session, &active_by_rule, &enabled, &rules_by_id)
            .await;
        Ok(())
    }

    async fn notify_pending_fires(
        &self,
        session: &mut EvaluationSession,
        rules_by_id: &HashMap<Uuid, &AlertRule>,
        inhibit_rules: &[InhibitRule],
        pending_fires: Vec<PendingFire>,
    ) {
        let sources = session.active_event_labels().await.unwrap_or_default();
        for fire in pending_fires {
            let Some(rule) = rules_by_id.get(&fire.rule_id) else {
                continue;
            };
            if is_inhibited(inhibit_rules, &sources, &fire.labels) {
                if let Err(error) = session.set_inhibited(fire.event_id).await {
                    tracing::warn!(target: "alerting", event_id = %fire.event_id, %error, "标记抑制失败");
                }
                continue;
            }
            if fire.silenced {
                continue;
            }
            self.notifier.dispatch(
                &rule.notify,
                &AlertNotification {
                    status: "firing",
                    event_id: fire.event_id,
                    rule_id: rule.id,
                    rule: rule.name.clone(),
                    severity: rule.severity.clone(),
                    value: fire.value,
                    threshold: rule.threshold,
                    op: rule.op.as_str(),
                    labels: fire.labels,
                    started_at: fire.started_at,
                    resolved_at: None,
                    console_url: self.notifier.console_url(fire.event_id),
                },
            );
        }
    }

    async fn resolve_disabled(
        &self,
        session: &mut EvaluationSession,
        active_by_rule: &HashMap<Uuid, HashMap<String, ActiveEvent>>,
        enabled: &HashSet<Uuid>,
        rules_by_id: &HashMap<Uuid, &AlertRule>,
    ) {
        for (rule_id, events) in active_by_rule {
            if enabled.contains(rule_id) {
                continue;
            }
            for event in events.values() {
                match session.resolve(event.id).await {
                    Ok(Some(resolved)) => {
                        if let Some(rule) = rules_by_id.get(rule_id) {
                            self.notify_resolved(rule, &resolved);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(target: "alerting", event_id = %event.id, %error, "结束停用规则的告警事件失败");
                    }
                }
            }
        }
    }

    async fn samples(&self, rule: &AlertRule) -> Result<Vec<Sample>, String> {
        match rule.kind {
            RuleKind::Promql => {
                let client = self
                    .greptimedb
                    .as_ref()
                    .ok_or_else(|| "GreptimeDB 未配置，无法评估 PromQL 规则".to_string())?;
                let series = client
                    .query_instant_vector(&rule.expr)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(series
                    .into_iter()
                    .map(|(mut labels, value)| {
                        labels.remove("__name__");
                        Sample { labels, value }
                    })
                    .collect())
            }
            RuleKind::Olap => {
                let mut expr: OlapExpr = serde_json::from_str(&rule.expr)
                    .map_err(|e| format!("OLAP 表达式无法解析: {e}"))?;
                // 落库表达式只保留窗长；评估时对齐到当前时刻。
                expr.query.rebase_window_to_now(Utc::now().timestamp());
                let rows = self
                    .clickhouse
                    .olap_query(&expr.query)
                    .await
                    .map_err(|e| e.to_string())?;
                let rows = serde_json::to_value(rows).map_err(|e| e.to_string())?;
                Ok(samples_from_rows(
                    &rows,
                    expr.value_field.as_deref().unwrap_or("value"),
                ))
            }
        }
    }

    async fn apply_non_fire(&self, session: &mut EvaluationSession, rule: &AlertRule, action: Action) {
        match action {
            Action::Fire { .. } => {}
            Action::Refresh { event_id, value } => {
                if let Err(error) = session.refresh_value(event_id, value).await {
                    tracing::warn!(target: "alerting", %event_id, %error, "刷新告警事件值失败");
                }
            }
            Action::Resolve { event_id } => match session.resolve(event_id).await {
                Ok(Some(resolved)) => {
                    tracing::info!(target: "alerting", rule = %rule.name, %event_id, "告警恢复");
                    self.notify_resolved(rule, &resolved);
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(target: "alerting", %event_id, %error, "恢复告警事件失败");
                }
            },
        }
    }

    async fn exemplar_trace_ids(
        &self,
        rule: &AlertRule,
        labels: &std::collections::BTreeMap<String, String>,
    ) -> Vec<String> {
        let query = exemplar_query(&rule.expr, labels, Utc::now().timestamp());
        match self.clickhouse.olap_query(&query).await {
            Ok(OlapRows::Exemplars(rows)) => {
                let mut ids: Vec<String> = Vec::new();
                for row in rows {
                    if !row.trace_id.is_empty() && !ids.contains(&row.trace_id) {
                        ids.push(row.trace_id);
                    }
                }
                ids
            }
            Ok(_) => Vec::new(),
            Err(error) => {
                tracing::warn!(target: "alerting", rule = %rule.name, %error, "查询告警样例 trace 失败");
                Vec::new()
            }
        }
    }

    fn notify_resolved(&self, rule: &AlertRule, resolved: &rule::ResolvedEvent) {
        self.notifier.dispatch(
            &rule.notify,
            &AlertNotification {
                status: "resolved",
                event_id: resolved.id,
                rule_id: resolved.rule_id,
                rule: rule.name.clone(),
                severity: rule.severity.clone(),
                value: resolved.value,
                threshold: rule.threshold,
                op: rule.op.as_str(),
                labels: resolved.labels.clone(),
                started_at: resolved.started_at,
                resolved_at: Some(resolved.resolved_at),
                console_url: self.notifier.console_url(resolved.id),
            },
        );
    }
}

/// 样例 trace 窗口半宽（秒）。
const EXEMPLAR_WINDOW_SECS: u32 = 300;
/// 每个事件附带的样例 trace 数。
const EXEMPLAR_LIMIT: u32 = 5;

/// 告警样例 trace 查询：表达式涉及上游耗时时按 `upstream_service_time_ms` 取样，否则按总耗时。
fn exemplar_query(
    expr: &str,
    labels: &std::collections::BTreeMap<String, String>,
    now: i64,
) -> OlapQuery {
    let metric = if expr.contains("upstream") {
        ExemplarMetric::UpstreamServiceTimeMs
    } else {
        ExemplarMetric::DurationMs
    };
    let label = |key: &str| labels.get(key).filter(|v| !v.is_empty()).cloned();
    OlapQuery::ExemplarTraces(ExemplarParams {
        metric,
        ts: u32::try_from(now).unwrap_or(u32::MAX),
        window_secs: Some(EXEMPLAR_WINDOW_SECS),
        limit: Some(EXEMPLAR_LIMIT),
        filters: Filters {
            tenant_code: label("tenant_code"),
            product_id: label("product_id"),
            supplier_code: label("supplier_code"),
            channel_code: label("channel_code"),
        },
    })
}

struct PendingFire {
    event_id: Uuid,
    rule_id: Uuid,
    labels: BTreeMap<String, String>,
    value: f64,
    started_at: chrono::DateTime<Utc>,
    silenced: bool,
}

/// 启动告警评估循环（持有 PG advisory lock 的实例执行）。
///
/// # Errors
///
/// SMTP / 配置校验失败时返回错误。
pub fn spawn(
    config: &AlertingConfig,
    greptimedb: Option<Arc<GreptimeDBClient>>,
    clickhouse: Arc<ClickHouseClient>,
    pool: Arc<mazbri_core::sqlx::PgPool>,
) -> anyhow::Result<()> {
    if !config.enabled {
        tracing::info!(target: "alerting", "告警评估已停用");
        return Ok(());
    }
    config.validate().map_err(anyhow::Error::msg)?;
    let notifier = Notifier::new(
        Duration::from_secs(config.webhook_timeout_secs),
        config.smtp.as_ref(),
        config.console_base_url.clone(),
    )
    .map_err(anyhow::Error::msg)?;
    let evaluator = Evaluator::new(AlertStore::new(pool), greptimedb, clickhouse, notifier);
    let interval = Duration::from_secs(config.interval_secs);
    tokio::spawn(evaluator.run(interval));
    tracing::info!(target: "alerting", interval_secs = config.interval_secs, smtp = config.smtp.is_some(), "告警评估已启动");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn exemplar_query_uses_labels_and_metric() {
        let labels = BTreeMap::from([
            ("supplier_code".to_string(), "upupt-rest".to_string()),
            ("instance".to_string(), "x".to_string()),
            ("product_id".to_string(), String::new()),
        ]);
        let OlapQuery::ExemplarTraces(p) =
            exemplar_query("upstream_timeout_ratio > 0.1", &labels, 1_700_000_000)
        else {
            panic!("expected exemplar query");
        };
        assert_eq!(p.metric, ExemplarMetric::UpstreamServiceTimeMs);
        assert_eq!(p.ts, 1_700_000_000);
        assert_eq!(p.filters.supplier_code.as_deref(), Some("upupt-rest"));
        assert!(p.filters.product_id.is_none());
        let OlapQuery::ExemplarTraces(p) = exemplar_query("p99_latency_ms", &labels, 1) else {
            panic!("expected exemplar query");
        };
        assert_eq!(p.metric, ExemplarMetric::DurationMs);
    }
}
