//! 告警状态机：持续时间（`for_seconds`）判定与指纹去重，纯逻辑、可单测。
use super::rule::{ActiveEvent, AlertRule, Sample, fingerprint};
use chrono::{DateTime, Duration, Utc};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

/// 一轮评估对数据库需要执行的动作。
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// 新建 firing 事件。
    Fire {
        rule_id: Uuid,
        fingerprint: String,
        labels: BTreeMap<String, String>,
        value: f64,
        started_at: DateTime<Utc>,
    },
    /// 活跃事件仍超阈值，刷新当前值。
    Refresh { event_id: Uuid, value: f64 },
    /// 活跃事件已恢复（或规则被停用 / 删除）。
    Resolve { event_id: Uuid },
}

/// 跨评估轮次保存每条规则「待定」样本首次超阈值的时刻。
#[derive(Debug, Default)]
pub struct Engine {
    pending: HashMap<Uuid, HashMap<String, DateTime<Utc>>>,
}

impl Engine {
    /// 对一条查询成功的启用规则生成动作；`active` 为该规则的活跃事件（按指纹索引）。
    pub fn plan_rule(
        &mut self,
        rule: &AlertRule,
        samples: &[Sample],
        active: &HashMap<String, ActiveEvent>,
        now: DateTime<Utc>,
    ) -> Vec<Action> {
        let pending = self.pending.entry(rule.id).or_default();
        let mut actions = Vec::new();
        let mut breaching = HashSet::new();
        for sample in samples {
            if !rule.op.breached(sample.value, rule.threshold) {
                continue;
            }
            let fp = fingerprint(rule.id, &sample.labels);
            if !breaching.insert(fp.clone()) {
                continue;
            }
            if let Some(event) = active.get(&fp) {
                pending.remove(&fp);
                if (event.value - sample.value).abs() > f64::EPSILON {
                    actions.push(Action::Refresh {
                        event_id: event.id,
                        value: sample.value,
                    });
                }
                continue;
            }
            let first_seen = *pending.entry(fp.clone()).or_insert(now);
            if now - first_seen >= Duration::seconds(rule.for_seconds) {
                pending.remove(&fp);
                let mut labels = rule.labels.clone();
                labels.extend(sample.labels.clone());
                actions.push(Action::Fire {
                    rule_id: rule.id,
                    fingerprint: fp,
                    labels,
                    value: sample.value,
                    started_at: first_seen,
                });
            }
        }
        pending.retain(|fp, _| breaching.contains(fp));
        for (fp, event) in active {
            if !breaching.contains(fp) {
                actions.push(Action::Resolve { event_id: event.id });
            }
        }
        actions
    }

    /// 丢弃不在 `enabled` 中的规则的待定状态（规则停用或删除）。
    pub fn retain_rules(&mut self, enabled: &HashSet<Uuid>) {
        self.pending.retain(|rule_id, _| enabled.contains(rule_id));
    }

    /// 某条规则当前待定的样本数。
    #[must_use]
    pub fn pending_count(&self, rule_id: Uuid) -> usize {
        self.pending.get(&rule_id).map_or(0, HashMap::len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alerting::rule::{CompareOp, NotifyConfig, RuleKind};

    fn rule(for_seconds: i64) -> AlertRule {
        AlertRule {
            id: Uuid::from_u128(1),
            name: "熔断打开".to_string(),
            kind: RuleKind::Promql,
            expr: "max by (name) (circuit_breaker_state)".to_string(),
            threshold: 2.0,
            op: CompareOp::Ge,
            for_seconds,
            severity: "critical".to_string(),
            labels: BTreeMap::from([("team".to_string(), "ops".to_string())]),
            notify: NotifyConfig::default(),
            enabled: true,
        }
    }

    fn sample(name: &str, value: f64) -> Sample {
        Sample {
            labels: BTreeMap::from([("name".to_string(), name.to_string())]),
            value,
        }
    }

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + secs, 0).expect("timestamp")
    }

    #[test]
    fn fires_immediately_when_for_is_zero() {
        let mut engine = Engine::default();
        let actions = engine.plan_rule(&rule(0), &[sample("p1", 2.0)], &HashMap::new(), at(0));
        assert_eq!(actions.len(), 1);
        let Action::Fire {
            labels, started_at, ..
        } = &actions[0]
        else {
            panic!("expected fire, got {actions:?}");
        };
        assert_eq!(labels.get("team").map(String::as_str), Some("ops"));
        assert_eq!(labels.get("name").map(String::as_str), Some("p1"));
        assert_eq!(*started_at, at(0));
    }

    #[test]
    fn waits_for_duration_and_keeps_first_seen() {
        let mut engine = Engine::default();
        let r = rule(60);
        let none = HashMap::new();
        assert_eq!(
            engine.plan_rule(&r, &[sample("p1", 2.0)], &none, at(0)),
            Vec::<Action>::new()
        );
        assert_eq!(engine.pending_count(r.id), 1);
        assert_eq!(
            engine.plan_rule(&r, &[sample("p1", 2.0)], &none, at(30)),
            Vec::<Action>::new()
        );
        let actions = engine.plan_rule(&r, &[sample("p1", 2.0)], &none, at(60));
        assert!(matches!(&actions[..], [Action::Fire { started_at, .. }] if *started_at == at(0)));
        assert_eq!(engine.pending_count(r.id), 0);
    }

    #[test]
    fn recovery_before_duration_resets_pending() {
        let mut engine = Engine::default();
        let r = rule(60);
        let none = HashMap::new();
        engine.plan_rule(&r, &[sample("p1", 2.0)], &none, at(0));
        engine.plan_rule(&r, &[sample("p1", 0.0)], &none, at(30));
        assert_eq!(engine.pending_count(r.id), 0);
        assert_eq!(
            engine.plan_rule(&r, &[sample("p1", 2.0)], &none, at(60)),
            Vec::<Action>::new()
        );
    }

    #[test]
    fn active_event_is_deduplicated_then_resolved() {
        let mut engine = Engine::default();
        let r = rule(0);
        let s = sample("p1", 2.0);
        let fp = fingerprint(r.id, &s.labels);
        let event = ActiveEvent {
            id: Uuid::from_u128(9),
            rule_id: r.id,
            fingerprint: fp.clone(),
            value: 2.0,
        };
        let active = HashMap::from([(fp, event)]);
        assert_eq!(
            engine.plan_rule(&r, std::slice::from_ref(&s), &active, at(0)),
            Vec::<Action>::new()
        );
        let refreshed = engine.plan_rule(&r, &[sample("p1", 3.0)], &active, at(30));
        assert_eq!(
            refreshed,
            vec![Action::Refresh {
                event_id: Uuid::from_u128(9),
                value: 3.0
            }]
        );
        let resolved = engine.plan_rule(&r, &[], &active, at(60));
        assert_eq!(
            resolved,
            vec![Action::Resolve {
                event_id: Uuid::from_u128(9)
            }]
        );
    }

    #[test]
    fn duplicate_series_fire_once_and_disabled_rules_are_forgotten() {
        let mut engine = Engine::default();
        let r = rule(0);
        let actions = engine.plan_rule(
            &r,
            &[sample("p1", 2.0), sample("p1", 2.0), sample("p2", 2.0)],
            &HashMap::new(),
            at(0),
        );
        assert_eq!(actions.len(), 2);
        let slow = rule(600);
        engine.plan_rule(&slow, &[sample("p1", 2.0)], &HashMap::new(), at(0));
        assert_eq!(engine.pending_count(slow.id), 1);
        engine.retain_rules(&HashSet::new());
        assert_eq!(engine.pending_count(slow.id), 0);
    }
}
