//! 告警规则模型、阈值比较与样本指纹。
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// 规则数据来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    /// `GreptimeDB` `PromQL` 即时查询，每条序列一个样本。
    Promql,
    /// `ClickHouse` OLAP 模板查询，每行一个样本。
    Olap,
}

impl RuleKind {
    /// 解析数据库中的 `kind` 列。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "promql" => Some(Self::Promql),
            "olap" => Some(Self::Olap),
            _ => None,
        }
    }
}

/// 阈值比较运算符。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
    Ne,
}

impl CompareOp {
    /// 解析数据库中的 `op` 列。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            ">" => Some(Self::Gt),
            ">=" => Some(Self::Ge),
            "<" => Some(Self::Lt),
            "<=" => Some(Self::Le),
            "==" => Some(Self::Eq),
            "!=" => Some(Self::Ne),
            _ => None,
        }
    }

    /// 运算符的规范字符串。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Eq => "==",
            Self::Ne => "!=",
        }
    }

    /// `value op threshold` 是否成立（非有限值一律不触发）。
    #[must_use]
    pub fn breached(self, value: f64, threshold: f64) -> bool {
        if !value.is_finite() || !threshold.is_finite() {
            return false;
        }
        match self {
            Self::Gt => value > threshold,
            Self::Ge => value >= threshold,
            Self::Lt => value < threshold,
            Self::Le => value <= threshold,
            Self::Eq => (value - threshold).abs() < f64::EPSILON,
            Self::Ne => (value - threshold).abs() >= f64::EPSILON,
        }
    }
}

/// 规则的通知配置（`alert_rules.notify`）。
/// `enabled=false`（默认）时只落库不外发；为 true 时 emails/webhooks 至少一项才应被管理面接受。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotifyConfig {
    /// 是否外发通知；关闭时评估与事件落库照常进行。
    #[serde(default)]
    pub enabled: bool,
    /// 触发与恢复时发送邮件的收件人地址列表。
    #[serde(default)]
    pub emails: Vec<String>,
    /// 触发与恢复时 POST JSON 的 webhook 地址列表。
    #[serde(default)]
    pub webhooks: Vec<String>,
}

/// 已加载的告警规则。
#[derive(Debug, Clone)]
pub struct AlertRule {
    pub id: Uuid,
    pub name: String,
    pub kind: RuleKind,
    pub expr: String,
    pub threshold: f64,
    pub op: CompareOp,
    pub for_seconds: i64,
    pub severity: String,
    pub labels: BTreeMap<String, String>,
    pub notify: NotifyConfig,
    pub enabled: bool,
}

/// 一次评估得到的单个样本。
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub labels: BTreeMap<String, String>,
    pub value: f64,
}

/// 数据库中处于活跃状态（firing）的事件。
#[derive(Debug, Clone)]
pub struct ActiveEvent {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub fingerprint: String,
    pub value: f64,
}

/// 已结束（resolved）的事件摘要，用于恢复通知。
#[derive(Debug, Clone)]
pub struct ResolvedEvent {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub labels: BTreeMap<String, String>,
    pub value: f64,
    pub started_at: DateTime<Utc>,
    pub resolved_at: DateTime<Utc>,
}

/// 规则 + 样本标签的稳定指纹（FNV-1a 64 位，十六进制），用于事件去重。
#[must_use]
pub fn fingerprint(rule_id: Uuid, labels: &BTreeMap<String, String>) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
    };
    feed(rule_id.as_bytes());
    for (key, value) in labels {
        feed(&[0xff]);
        feed(key.as_bytes());
        feed(&[0xfe]);
        feed(value.as_bytes());
    }
    format!("{hash:016x}")
}

/// OLAP 规则表达式：`{"query": {template, params}, "value_field": "..."}`。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OlapExpr {
    pub query: crate::clickhouse::olap::OlapQuery,
    /// 取值字段，缺省为 `value`。
    #[serde(default)]
    pub value_field: Option<String>,
}

/// 将 OLAP 结果行转换为样本：取值字段为数值，其余字符串字段作为标签。
#[must_use]
pub fn samples_from_rows(rows: &serde_json::Value, value_field: &str) -> Vec<Sample> {
    let Some(rows) = rows.as_array() else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let object = row.as_object()?;
            let value = object.get(value_field).and_then(json_number)?;
            let labels = object
                .iter()
                .filter(|(key, _)| key.as_str() != value_field)
                .filter_map(|(key, raw)| raw.as_str().map(|s| (key.clone(), s.to_string())))
                .collect();
            Some(Sample { labels, value })
        })
        .collect()
}

fn json_number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

/// 告警通知内容：webhook 以 JSON 发送，邮件据此渲染主题与正文。
#[derive(Debug, Clone, Serialize)]
pub struct AlertNotification {
    pub status: &'static str,
    pub event_id: Uuid,
    pub rule_id: Uuid,
    pub rule: String,
    pub severity: String,
    pub value: f64,
    pub threshold: f64,
    pub op: &'static str,
    pub labels: BTreeMap<String, String>,
    pub started_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn notify_config_defaults_and_rejects_unknown_keys() {
        let parsed: NotifyConfig = serde_json::from_value(serde_json::json!({
            "enabled": true,
            "emails": ["ops@example.com"],
            "webhooks": []
        }))
        .expect("valid notify");
        assert!(parsed.enabled);
        assert_eq!(parsed.emails, vec!["ops@example.com".to_string()]);
        let off: NotifyConfig =
            serde_json::from_value(serde_json::json!({"emails": [], "webhooks": []}))
                .expect("notify off default");
        assert!(!off.enabled);
        assert!(
            serde_json::from_value::<NotifyConfig>(
                serde_json::json!({"emails": [], "webhooks": [], "slack": "x"})
            )
            .is_err()
        );
    }

    #[test]
    fn compare_ops_follow_threshold_semantics() {
        assert!(CompareOp::Gt.breached(5.1, 5.0));
        assert!(!CompareOp::Gt.breached(5.0, 5.0));
        assert!(CompareOp::Ge.breached(2.0, 2.0));
        assert!(CompareOp::Lt.breached(29.0, 30.0));
        assert!(CompareOp::Le.breached(30.0, 30.0));
        assert!(CompareOp::Eq.breached(1.0, 1.0));
        assert!(CompareOp::Ne.breached(0.0, 1.0));
        assert!(!CompareOp::Gt.breached(f64::NAN, 0.0));
        assert!(!CompareOp::Lt.breached(f64::INFINITY, 1.0));
    }

    #[test]
    fn op_round_trips_through_strings() {
        for raw in [">", ">=", "<", "<=", "==", "!="] {
            assert_eq!(CompareOp::parse(raw).map(CompareOp::as_str), Some(raw));
        }
        assert!(CompareOp::parse("=>").is_none());
        assert_eq!(RuleKind::parse("olap"), Some(RuleKind::Olap));
        assert!(RuleKind::parse("sql").is_none());
    }

    #[test]
    fn fingerprint_is_stable_and_label_sensitive() {
        let rule = Uuid::from_u128(7);
        let a = labels(&[("name", "product:1"), ("service", "gw")]);
        let b = labels(&[("service", "gw"), ("name", "product:1")]);
        let c = labels(&[("name", "product:2"), ("service", "gw")]);
        assert_eq!(fingerprint(rule, &a), fingerprint(rule, &b));
        assert_ne!(fingerprint(rule, &a), fingerprint(rule, &c));
        assert_ne!(fingerprint(rule, &a), fingerprint(Uuid::from_u128(8), &a));
        assert_eq!(fingerprint(rule, &a).len(), 16);
    }

    #[test]
    fn olap_rows_become_samples() {
        let rows = serde_json::json!([
            {"key": "p1", "value": 12.5, "count": 3},
            {"key": "p2", "value": "7"},
            {"key": "p3"}
        ]);
        let samples = samples_from_rows(&rows, "value");
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].labels, labels(&[("key", "p1")]));
        assert!((samples[0].value - 12.5).abs() < f64::EPSILON);
        assert!((samples[1].value - 7.0).abs() < f64::EPSILON);
    }

    #[test]
    fn olap_expr_parses_template_query() {
        let expr: OlapExpr = serde_json::from_str(
            r#"{"query":{"template":"top_by_dim","params":{"dim":"product_id","start":0,"end":900}},"value_field":"value"}"#,
        )
        .expect("olap expr");
        assert_eq!(expr.value_field.as_deref(), Some("value"));
        assert!(serde_json::from_str::<OlapExpr>(r#"{"query":{"template":"nope"}}"#).is_err());
    }
}
