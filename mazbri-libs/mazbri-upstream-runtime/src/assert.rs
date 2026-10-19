//! Assertion engine for connector publish gates.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::template::extract_by_path;
use crate::types::OutboundResponse;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

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

fn evaluate_one(
    assertion: &Assertion,
    response: Option<&OutboundResponse>,
    latency_ms: u64,
) -> AssertionResult {
    match assertion {
        Assertion::StatusIn { values, name } => {
            let name = name.clone().unwrap_or_else(|| "status_in".into());
            let Some(resp) = response else {
                return fail(name, "no response");
            };
            if values.contains(&resp.status) {
                pass(name)
            } else {
                fail(name, &format!("status {} not in {:?}", resp.status, values))
            }
        }
        Assertion::StatusRange { min, max, name } => {
            let name = name
                .clone()
                .unwrap_or_else(|| format!("status_range_{min}_{max}"));
            let Some(resp) = response else {
                return fail(name, "no response");
            };
            if resp.status >= *min && resp.status <= *max {
                pass(name)
            } else {
                fail(
                    name,
                    &format!("status {} not in {}..={}", resp.status, min, max),
                )
            }
        }
        Assertion::LatencyMsLte { value, name } => {
            let name = name
                .clone()
                .unwrap_or_else(|| format!("latency_ms_lte_{value}"));
            if latency_ms <= *value {
                pass(name)
            } else {
                fail(name, &format!("latency {latency_ms}ms > {value}ms"))
            }
        }
        Assertion::JsonpathExists { path, name } => {
            let name = name
                .clone()
                .unwrap_or_else(|| format!("jsonpath_exists:{path}"));
            match body_json(response) {
                Some(json) if extract_by_path(&json, path).is_some() => pass(name),
                Some(_) => fail(name, &format!("path {path} not found")),
                None => fail(name, "body is not JSON"),
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
            match body_json(response) {
                Some(json) => match extract_by_path(&json, path) {
                    Some(v) if &v == expected => pass(name),
                    Some(v) => fail(name, &format!("got {v}, expected {expected}")),
                    None => fail(name, &format!("path {path} not found")),
                },
                None => fail(name, "body is not JSON"),
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
            match body_json(response) {
                Some(json) => match extract_by_path(&json, path) {
                    Some(v) => {
                        let s = match v {
                            serde_json::Value::String(s) => s,
                            other => other.to_string(),
                        };
                        if s.contains(pattern.as_str()) {
                            pass(name)
                        } else {
                            fail(name, &format!("value does not contain '{pattern}'"))
                        }
                    }
                    None => fail(name, &format!("path {path} not found")),
                },
                None => fail(name, "body is not JSON"),
            }
        }
        Assertion::BodyContains { value, name } => {
            let name = name.clone().unwrap_or_else(|| "body_contains".into());
            let body = body_str(response);
            if body.contains(value) {
                pass(name)
            } else {
                fail(name, "body does not contain expected substring")
            }
        }
        Assertion::BodyNotContains { value, name } => {
            let name = name.clone().unwrap_or_else(|| "body_not_contains".into());
            let body = body_str(response);
            if !body.contains(value) {
                pass(name)
            } else {
                fail(name, "body unexpectedly contains substring")
            }
        }
        Assertion::HeaderEquals { key, value, name } => {
            let name = name
                .clone()
                .unwrap_or_else(|| format!("header_equals:{key}"));
            let Some(resp) = response else {
                return fail(name, "no response");
            };
            match find_header(&resp.headers, key) {
                Some(v) if v == value => pass(name),
                Some(v) => fail(name, &format!("header value mismatch: got '{v}'")),
                None => fail(name, "header missing"),
            }
        }
        Assertion::HeaderExists { key, name } => {
            let name = name
                .clone()
                .unwrap_or_else(|| format!("header_exists:{key}"));
            let Some(resp) = response else {
                return fail(name, "no response");
            };
            if find_header(&resp.headers, key).is_some() {
                pass(name)
            } else {
                fail(name, "header missing")
            }
        }
    }
}

fn pass(name: String) -> AssertionResult {
    AssertionResult {
        name,
        ok: true,
        message: None,
    }
}

fn fail(name: String, msg: &str) -> AssertionResult {
    AssertionResult {
        name,
        ok: false,
        message: Some(msg.to_string()),
    }
}

fn body_str(response: Option<&OutboundResponse>) -> String {
    response
        .map(|r| String::from_utf8_lossy(&r.body).to_string())
        .unwrap_or_default()
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
    }
}
