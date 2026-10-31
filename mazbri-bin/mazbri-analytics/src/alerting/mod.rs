//! 最小告警中心：周期评估 `alert_rules`，按持续时间与指纹去重写入 `alert_events`，
//! 触发 / 恢复时向规则配置的 webhook 发送通知。
pub mod engine;
pub mod notifier;
pub mod rule;
pub mod store;

use crate::clickhouse::ClickHouseClient;
use crate::greptimedb::GreptimeDBClient;
use chrono::Utc;
use engine::{Action, Engine};
use notifier::Notifier;
use rule::{ActiveEvent, AlertRule, OlapExpr, RuleKind, Sample, WebhookPayload, samples_from_rows};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use store::AlertStore;
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
        let rules = self.store.load_rules().await?;
        let mut active_by_rule: HashMap<Uuid, HashMap<String, ActiveEvent>> = HashMap::new();
        for event in self.store.active_events().await? {
            active_by_rule
                .entry(event.rule_id)
                .or_default()
                .insert(event.fingerprint.clone(), event);
        }
        let enabled: HashSet<Uuid> = rules.iter().filter(|r| r.enabled).map(|r| r.id).collect();
        self.engine.retain_rules(&enabled);
        let rules_by_id: HashMap<Uuid, &AlertRule> = rules.iter().map(|r| (r.id, r)).collect();
        let empty = HashMap::new();
        for rule in rules.iter().filter(|r| r.enabled) {
            let samples = match self.samples(rule).await {
                Ok(samples) => samples,
                Err(error) => {
                    tracing::warn!(target: "alerting", rule = %rule.name, %error, "告警规则查询失败，本轮跳过");
                    continue;
                }
            };
            let active = active_by_rule.get(&rule.id).unwrap_or(&empty);
            let actions = self.engine.plan_rule(rule, &samples, active, Utc::now());
            for action in actions {
                self.apply(rule, action).await;
            }
        }
        // 停用或已删除规则遗留的活跃事件直接结束。
        for (rule_id, events) in &active_by_rule {
            if enabled.contains(rule_id) {
                continue;
            }
            for event in events.values() {
                match self.store.resolve(event.id).await {
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
        Ok(())
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
                let expr: OlapExpr = serde_json::from_str(&rule.expr)
                    .map_err(|e| format!("OLAP 表达式无法解析: {e}"))?;
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

    async fn apply(&self, rule: &AlertRule, action: Action) {
        match action {
            Action::Fire {
                fingerprint,
                labels,
                value,
                started_at,
                ..
            } => match self
                .store
                .insert_firing(rule, &fingerprint, &labels, value, started_at)
                .await
            {
                Ok(Some(event_id)) => {
                    tracing::warn!(target: "alerting", rule = %rule.name, %event_id, value, "告警触发");
                    self.notifier.dispatch(
                        &rule.notify.webhooks,
                        &WebhookPayload {
                            status: "firing",
                            event_id,
                            rule_id: rule.id,
                            rule: rule.name.clone(),
                            severity: rule.severity.clone(),
                            value,
                            threshold: rule.threshold,
                            op: rule.op.as_str(),
                            labels,
                            started_at,
                            resolved_at: None,
                            console_url: self.notifier.console_url(event_id),
                        },
                    );
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(target: "alerting", rule = %rule.name, %error, "写入告警事件失败");
                }
            },
            Action::Refresh { event_id, value } => {
                if let Err(error) = self.store.refresh_value(event_id, value).await {
                    tracing::warn!(target: "alerting", %event_id, %error, "刷新告警事件值失败");
                }
            }
            Action::Resolve { event_id } => match self.store.resolve(event_id).await {
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

    fn notify_resolved(&self, rule: &AlertRule, resolved: &rule::ResolvedEvent) {
        self.notifier.dispatch(
            &rule.notify.webhooks,
            &WebhookPayload {
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

/// 按配置启动告警评估后台任务；未配置或停用时不启动。
///
/// # Errors
///
/// 配置无效、连接池或 HTTP 客户端构建失败时返回错误。
pub fn spawn(
    config: &AlertingConfig,
    greptimedb: Option<Arc<GreptimeDBClient>>,
    clickhouse: Arc<ClickHouseClient>,
) -> anyhow::Result<()> {
    if !config.enabled {
        tracing::info!(target: "alerting", "告警评估已停用");
        return Ok(());
    }
    config.validate().map_err(anyhow::Error::msg)?;
    let pool = mazbri_core::create_lazy_pool(&config.database, None)?;
    let notifier = Notifier::new(
        Duration::from_secs(config.webhook_timeout_secs),
        config.console_base_url.clone(),
    )?;
    let evaluator = Evaluator::new(AlertStore::new(pool), greptimedb, clickhouse, notifier);
    let interval = Duration::from_secs(config.interval_secs);
    tokio::spawn(evaluator.run(interval));
    tracing::info!(target: "alerting", interval_secs = config.interval_secs, "告警评估已启动");
    Ok(())
}
