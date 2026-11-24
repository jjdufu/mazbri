//! Assertion engine for connector publish gates.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::types::OutboundResponse;
use mazbri_core::json_path::extract_by_path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Assertion {
    StatusIn {
        values: Vec<u16>,
        #[serde(default)]
        name: Option<String>,
    },
    StatusRange {
        min: u16,
        max: u16,
        #[serde(default)]
        name: Option<String>,
    },
    LatencyMsLte {
        value: u64,
        #[serde(default)]
        name: Option<String>,
    },
    JsonpathExists {
        path: String,
        #[serde(default)]
        name: Option<String>,
    },
    JsonpathEquals {
        path: String,
        expected: serde_json::Value,
        #[serde(default)]
        name: Option<String>,
    },
    JsonpathMatches {
        path: String,
        /// Substring or simple pattern (contains).
        pattern: String,
        #[serde(default)]
        name: Option<String>,
    },
    BodyContains {
        value: String,
        #[serde(default)]
        name: Option<String>,
    },
    BodyNotContains {
        value: String,
        #[serde(default)]
        name: Option<String>,
    },
    HeaderEquals {
        key: String,
        value: String,
        #[serde(default)]
        name: Option<String>,
    },
    HeaderExists {
        key: String,
        #[serde(default)]
        name: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionResult {
    pub name: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<serde_json::Value>,
}

#[must_use]
pub fn evaluate_assertions(
    assertions: &[Assertion],
    response: Option<&OutboundResponse>,
    latency_ms: u64,
) -> Vec<AssertionResult> {
    assertions
        .iter()
        .map(|a| evaluate_one(a, response, latency_ms))
        .collect()
}

fn evaluate_jsonpath(
    assertion: &Assertion,
    response: Option<&OutboundResponse>,
) -> AssertionResult {
    match assertion {
        Assertion::JsonpathExists { path, name } => {
            let name = name
                .clone()
                .unwrap_or_else(|| format!("jsonpath_exists:{path}"));
            let exp = Some(serde_json::json!(format!("exists({path})")));
            match body_json(response) {
                Some(json) => match extract_by_path(&json, path) {
                    Some(v) => pass_with(name, exp, Some(v)),
                    None => fail_with(name, &format!("path {path} not found"), exp, Some(serde_json::Value::Null)),
                },
                None => fail_with(name, "body is not JSON", exp, None),
            }
        }
        Assertion::JsonpathEquals {
            path,
            expected,
            name,
        } => {
            let name = name
                .clone()
                .unwrap_or_else(|| format!("jsonpath_equals:{path}"));
            let exp = Some(expected.clone());
            match body_json(response) {
                Some(json) => match extract_by_path(&json, path) {
                    Some(v) if &v == expected => pass_with(name, exp, Some(v)),
                    Some(v) => fail_with(
                        name,
                        &format!("got {v}, expected {expected}"),
                        exp,
                        Some(v),
                    ),
                    None => fail_with(
                        name,
                        &format!("path {path} not found"),
                        exp,
                        Some(serde_json::Value::Null),
                    ),
                },
                None => fail_with(name, "body is not JSON", exp, None),
            }
        }
        Assertion::JsonpathMatches {
            path,
            pattern,
            name,
        } => {
            let name = name
                .clone()
                .unwrap_or_else(|| format!("jsonpath_matches:{path}"));
            let exp = Some(serde_json::json!(pattern));
            match body_json(response) {
                Some(json) => match extract_by_path(&json, path) {
                    Some(v) => {
                        let s = match &v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        if s.contains(pattern.as_str()) {
                            pass_with(name, exp, Some(v))
                        } else {
                            fail_with(
                                name,
                                &format!("value does not contain '{pattern}'"),
                                exp,
                                Some(v),
                            )
                        }
                    }
                    None => fail_with(
                        name,
                        &format!("path {path} not found"),
                        exp,
                        Some(serde_json::Value::Null),
                    ),
                },
                None => fail_with(name, "body is not JSON", exp, None),
            }
        }
        _ => unreachable!("evaluate_jsonpath only handles jsonpath assertions"),
    }
}

fn evaluate_one(
    assertion: &Assertion,
    response: Option<&OutboundResponse>,
    latency_ms: u64,
) -> AssertionResult {
    match assertion {
        Assertion::StatusIn { values, name } => {
            let name = name.clone().unwrap_or_else(|| "status_in".into());
            let exp = Some(serde_json::json!(values));
            let Some(resp) = response else {
                return fail_with(name, "no response", exp, None);
            };
            let act = Some(serde_json::json!(resp.status));
            if values.contains(&resp.status) {
                pass_with(name, exp, act)
            } else {
                fail_with(
                    name,
                    &format!("status {} not in {:?}", resp.status, values),
                    exp,
                    act,
                )
            }
        }
        Assertion::StatusRange { min, max, name } => {
            let name = name
                .clone()
                .unwrap_or_else(|| format!("status_range_{min}_{max}"));
            let exp = Some(serde_json::json!(format!("{min}..={max}")));
            let Some(resp) = response else {
                return fail_with(name, "no response", exp, None);
            };
            let act = Some(serde_json::json!(resp.status));
            if resp.status >= *min && resp.status <= *max {
                pass_with(name, exp, act)
            } else {
                fail_with(
                    name,
                    &format!("status {} not in {}..={}", resp.status, min, max),
                    exp,
                    act,
                )
            }
        }
        Assertion::LatencyMsLte { value, name } => {
            let name = name
                .clone()
                .unwrap_or_else(|| format!("latency_ms_lte_{value}"));
            let exp = Some(serde_json::json!(format!("<={value}ms")));
            let act = Some(serde_json::json!(format!("{latency_ms}ms")));
            if latency_ms <= *value {
                pass_with(name, exp, act)
            } else {
                fail_with(name, &format!("latency {latency_ms}ms > {value}ms"), exp, act)
            }
        }
        Assertion::JsonpathExists { .. }
        | Assertion::JsonpathEquals { .. }
        | Assertion::JsonpathMatches { .. } => evaluate_jsonpath(assertion, response),
        Assertion::BodyContains { value, name } => evaluate_body_contains(value, name.as_ref(), response),
        Assertion::BodyNotContains { value, name } => {
            evaluate_body_not_contains(value, name.as_ref(), response)
        }
        Assertion::HeaderEquals { key, value, name } => {
            evaluate_header_equals(key, value, name.as_ref(), response)
        }
        Assertion::HeaderExists { key, name } => {
            evaluate_header_exists(key, name.as_ref(), response)
        }
    }
}

fn evaluate_body_contains(
    value: &str,
    name: Option<&String>,
    response: Option<&OutboundResponse>,
) -> AssertionResult {
    let name = name.cloned().unwrap_or_else(|| "body_contains".into());
    let body = body_str(response);
    let exp = Some(serde_json::json!(value));
    let preview: String = body.chars().take(160).collect();
    if body.contains(value) {
        pass_with(name, exp, Some(serde_json::json!(preview)))
    } else {
        fail_with(
            name,
            "body does not contain expected substring",
            exp,
            Some(serde_json::json!(preview)),
        )
    }
}

fn evaluate_body_not_contains(
    value: &str,
    name: Option<&String>,
    response: Option<&OutboundResponse>,
) -> AssertionResult {
    let name = name.cloned().unwrap_or_else(|| "body_not_contains".into());
    let body = body_str(response);
    let exp = Some(serde_json::json!(format!("!contains({value})")));
    let preview: String = body.chars().take(160).collect();
    if body.contains(value) {
        fail_with(
            name,
            "body unexpectedly contains substring",
            exp,
            Some(serde_json::json!(preview)),
        )
    } else {
        pass_with(name, exp, Some(serde_json::json!(preview)))
    }
}

fn evaluate_header_equals(
    key: &str,
    value: &str,
    name: Option<&String>,
    response: Option<&OutboundResponse>,
) -> AssertionResult {
    let name = name
        .cloned()
        .unwrap_or_else(|| format!("header_equals:{key}"));
    let exp = Some(serde_json::json!(value));
    let Some(resp) = response else {
        return fail_with(name, "no response", exp, None);
    };
    match find_header(&resp.headers, key) {
        Some(v) if v == value => pass_with(name, exp, Some(serde_json::json!(v))),
        Some(v) => fail_with(
            name,
            &format!("header value mismatch: got '{v}'"),
            exp,
            Some(serde_json::json!(v)),
        ),
        None => fail_with(name, "header missing", exp, Some(serde_json::Value::Null)),
    }
}

fn evaluate_header_exists(
    key: &str,
    name: Option<&String>,
    response: Option<&OutboundResponse>,
) -> AssertionResult {
    let name = name
        .cloned()
        .unwrap_or_else(|| format!("header_exists:{key}"));
    let exp = Some(serde_json::json!(format!("exists({key})")));
    let Some(resp) = response else {
        return fail_with(name, "no response", exp, None);
    };
    if let Some(v) = find_header(&resp.headers, key) {
        pass_with(name, exp, Some(serde_json::json!(v)))
    } else {
        fail_with(name, "header missing", exp, Some(serde_json::Value::Null))
    }
}

fn pass_with(
    name: String,
    expected: Option<serde_json::Value>,
    actual: Option<serde_json::Value>,
) -> AssertionResult {
    AssertionResult {
        name,
        ok: true,
        message: None,
        expected,
        actual,
    }
}

fn fail_with(
    name: String,
    msg: &str,
    expected: Option<serde_json::Value>,
    actual: Option<serde_json::Value>,
) -> AssertionResult {
    AssertionResult {
        name,
        ok: false,
        message: Some(msg.to_string()),
        expected,
        actual,
    }
}

fn body_str(response: Option<&OutboundResponse>) -> String {
    response.map_or_default(|r| String::from_utf8_lossy(&r.body).to_string())
}

fn body_json(response: Option<&OutboundResponse>) -> Option<serde_json::Value> {
    let s = body_str(response);
    serde_json::from_str(&s).ok()
}

fn find_header<'a>(headers: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(status: u16, body: &str) -> OutboundResponse {
        OutboundResponse {
            status,
            headers: HashMap::new(),
            body: body.as_bytes().to_vec(),
            final_url: "https://example.com/x".into(),
            method: "GET".into(),
            path: "/x".into(),
            protocol: "HTTP/1.1".into(),
            wire_duration: std::time::Duration::ZERO,
        }
    }

    #[test]
    fn status_2xx() {
        let a = Assertion::StatusRange {
            min: 200,
            max: 299,
            name: None,
        };
        let r = evaluate_one(&a, Some(&resp(200, "{}")), 10);
        assert!(r.ok);
    }

    #[test]
    fn jsonpath_equals() {
        let a = Assertion::JsonpathEquals {
            path: "$.id".into(),
            expected: serde_json::json!(1),
            name: None,
        };
        let r = evaluate_one(&a, Some(&resp(200, r#"{"id":1}"#)), 5);
        assert!(r.ok);
        assert_eq!(r.expected, Some(serde_json::json!(1)));
        assert_eq!(r.actual, Some(serde_json::json!(1)));

        let r_fail = evaluate_one(&a, Some(&resp(200, r#"{"id":2}"#)), 5);
        assert!(!r_fail.ok);
        assert_eq!(r_fail.expected, Some(serde_json::json!(1)));
        assert_eq!(r_fail.actual, Some(serde_json::json!(2)));
    }
}
