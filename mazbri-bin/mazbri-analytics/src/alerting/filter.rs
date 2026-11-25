//! 静默与抑制判定（纯逻辑，可单测）。
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeMap;

/// 一条活跃静默窗。
#[derive(Debug, Clone)]
pub struct Silence {
    pub matchers: Value,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

/// 一条启用的抑制规则。
#[derive(Debug, Clone)]
pub struct InhibitRule {
    pub source_matchers: Value,
    pub target_matchers: Value,
    pub equal_labels: Vec<String>,
}

/// 用于抑制判定的活跃源事件标签。
#[derive(Debug, Clone)]
pub struct ActiveLabels {
    pub event_id: uuid::Uuid,
    pub labels: BTreeMap<String, String>,
}

/// `matchers` 为等值对象：每个 key=value 都必须出现在 labels 中；空对象匹配全部。
#[must_use]
pub fn matchers_match(matchers: &Value, labels: &BTreeMap<String, String>) -> bool {
    let Some(obj) = matchers.as_object() else {
        return false;
    };
    if obj.is_empty() {
        return true;
    }
    obj.iter().all(|(k, v)| {
        let Some(want) = v.as_str() else {
            return false;
        };
        labels.get(k).map(String::as_str) == Some(want)
    })
}

/// 当前时刻是否落在静默窗且标签匹配。
#[must_use]
pub fn is_silenced(silences: &[Silence], labels: &BTreeMap<String, String>, now: DateTime<Utc>) -> bool {
    silences.iter().any(|s| {
        now >= s.starts_at && now < s.ends_at && matchers_match(&s.matchers, labels)
    })
}

/// 若存在匹配 `source_matchers` 的其他活跃源（排除目标事件自身），且目标标签匹配 `target_matchers`，
/// 且 `equal_labels` 上取值相等，则抑制目标。
#[must_use]
pub fn is_inhibited(
    rules: &[InhibitRule],
    sources: &[ActiveLabels],
    target_event_id: uuid::Uuid,
    target: &BTreeMap<String, String>,
) -> bool {
    for rule in rules {
        if !matchers_match(&rule.target_matchers, target) {
            continue;
        }
        for src in sources {
            if src.event_id == target_event_id {
                continue;
            }
            if !matchers_match(&rule.source_matchers, &src.labels) {
                continue;
            }
            let equal_ok = rule.equal_labels.iter().all(|key| {
                match (src.labels.get(key), target.get(key)) {
                    (Some(a), Some(b)) => a == b,
                    (None, None) => true,
                    _ => false,
                }
            });
            if equal_ok {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn labels(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000 + secs, 0).expect("ts")
    }

    #[test]
    fn empty_matchers_match_all() {
        assert!(matchers_match(&json!({}), &labels(&[("a", "1")])));
    }

    #[test]
    fn equality_matchers() {
        let m = json!({"product_id": "A", "category": "business"});
        assert!(matchers_match(
            &m,
            &labels(&[("product_id", "A"), ("category", "business"), ("x", "y")])
        ));
        assert!(!matchers_match(&m, &labels(&[("product_id", "B")])));
    }

    #[test]
    fn silence_window() {
        let s = Silence {
            matchers: json!({"product_id": "A"}),
            starts_at: at(0),
            ends_at: at(600),
        };
        let labs = labels(&[("product_id", "A")]);
        assert!(is_silenced(std::slice::from_ref(&s), &labs, at(0)));
        assert!(is_silenced(std::slice::from_ref(&s), &labs, at(599)));
        assert!(!is_silenced(std::slice::from_ref(&s), &labs, at(600)));
        assert!(!is_silenced(
            std::slice::from_ref(&s),
            &labels(&[("product_id", "B")]),
            at(100)
        ));
    }

    #[test]
    fn inhibit_breaker_suppresses_error_rate_same_product() {
        let rule = InhibitRule {
            source_matchers: json!({"category": "upstream"}),
            target_matchers: json!({"category": "business"}),
            equal_labels: vec!["product_id".into()],
        };
        let src_id = Uuid::from_u128(1);
        let dst_id = Uuid::from_u128(2);
        let sources = vec![ActiveLabels {
            event_id: src_id,
            labels: labels(&[("category", "upstream"), ("product_id", "A"), ("name", "cb")]),
        }];
        let target = labels(&[("category", "business"), ("product_id", "A")]);
        assert!(is_inhibited(
            std::slice::from_ref(&rule),
            &sources,
            dst_id,
            &target
        ));
        let other = labels(&[("category", "business"), ("product_id", "B")]);
        assert!(!is_inhibited(
            std::slice::from_ref(&rule),
            &sources,
            dst_id,
            &other
        ));
    }

    #[test]
    fn inhibit_excludes_self_when_source_and_target_match_same_event() {
        let broad_rule = InhibitRule {
            source_matchers: json!({"category": "upstream"}),
            target_matchers: json!({"category": "upstream"}),
            equal_labels: vec!["product_id".into()],
        };
        let self_id = Uuid::from_u128(42);
        let peer_id = Uuid::from_u128(99);
        let event_labels = labels(&[("category", "upstream"), ("product_id", "A")]);
        let only_self = vec![ActiveLabels {
            event_id: self_id,
            labels: event_labels.clone(),
        }];
        assert!(
            !is_inhibited(
                std::slice::from_ref(&broad_rule),
                &only_self,
                self_id,
                &event_labels
            ),
            "an event must never inhibit itself"
        );

        let with_peer = vec![
            ActiveLabels {
                event_id: self_id,
                labels: event_labels.clone(),
            },
            ActiveLabels {
                event_id: peer_id,
                labels: event_labels.clone(),
            },
        ];
        assert!(is_inhibited(
            std::slice::from_ref(&broad_rule),
            &with_peer,
            self_id,
            &event_labels
        ));
    }
}
