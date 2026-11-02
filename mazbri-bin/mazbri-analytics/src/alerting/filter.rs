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

/// 若存在匹配 `source_matchers` 的活跃源，且目标标签匹配 `target_matchers`，
/// 且 `equal_labels` 上取值相等，则抑制目标。
#[must_use]
pub fn is_inhibited(
    rules: &[InhibitRule],
    sources: &[ActiveLabels],
    target: &BTreeMap<String, String>,
) -> bool {
    for rule in rules {
        if !matchers_match(&rule.target_matchers, target) {
            continue;
        }
        for src in sources {
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
        let sources = vec![ActiveLabels {
            labels: labels(&[("category", "upstream"), ("product_id", "A"), ("name", "cb")]),
        }];
        let target = labels(&[("category", "business"), ("product_id", "A")]);
        assert!(is_inhibited(std::slice::from_ref(&rule), &sources, &target));
        let other = labels(&[("category", "business"), ("product_id", "B")]);
        assert!(!is_inhibited(std::slice::from_ref(&rule), &sources, &other));
    }

}
