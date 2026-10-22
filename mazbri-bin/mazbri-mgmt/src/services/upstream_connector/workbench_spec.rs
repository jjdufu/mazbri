//! Single workbench spec builder (D33): run / publish / `transform_preview`
//! assemble `ChannelEgressSpec` from Tab fields + `workbench_env`, never from
//! `ep.egress` or the browser `RunRequestBody`.

use mazbri_core::{
    ADAPTER_HTTP_JSON, AuthFlowConfig, ChannelEgressSpec, SupplierConfig, validate_egress_spec,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

use super::{ApiError, DraftGateItem, inherit_requires_session_profile, merge_auth_from_supplier};

/// PR2: leftover workbench `{{ident}}` on `WORKBENCH_ENV_SURFACE` is 4xx.
const FAIL_ON_UNRESOLVED_VARS: bool = true;

const EXECUTE_PREFIXES: [&str; 3] = ["request.", "response.", "credential."];

#[derive(Debug)]
pub struct BuiltWorkbenchSpec {
    pub spec: ChannelEgressSpec,
    pub payload: Value,
    pub fingerprint: String,
}

pub fn build_workbench_spec(
    endpoint: &Value,
    env: &HashMap<String, String>,
    supplier_cfg: Option<&SupplierConfig>,
    upstream_url: &str,
) -> Result<BuiltWorkbenchSpec, ApiError> {
    let expanded_ep = expand_endpoint(endpoint, env)?;
    let expanded_url = apply_env_str(upstream_url, env);

    let mut cfg_owned = supplier_cfg.cloned();
    if let Some(ref mut cfg) = cfg_owned {
        apply_env_to_supplier_config(cfg, env);
    }
    let cfg_ref = cfg_owned.as_ref();

    let spec = assemble_spec(&expanded_ep, &expanded_url, cfg_ref)?;
    let payload = payload_from_endpoint(&expanded_ep);

    if FAIL_ON_UNRESOLVED_VARS {
        let leftover =
            unresolved_idents_in(&leftover_scan_json(&expanded_url, cfg_ref, &spec, &payload));
        if !leftover.is_empty() {
            return Err(ApiError::validation(format!(
                "UNRESOLVED_VARS: {}",
                leftover.join(", ")
            )));
        }
    }

    let cred_keys = cred_keys_of(supplier_cfg);
    let fingerprint = fingerprint_workbench(&expanded_url, &spec, &payload, &cred_keys)?;

    Ok(BuiltWorkbenchSpec {
        spec,
        payload,
        fingerprint,
    })
}

fn env_value_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Accepts a legacy `{ "k": "v" }` map or a `KvRow` array
/// (`[{ key, value, desc, enabled }, …]`). Disabled / blank keys are omitted.
pub(crate) fn parse_workbench_env(value: &Value) -> HashMap<String, String> {
    let mut out = HashMap::new();
    match value {
        Value::Array(items) => {
            for item in items {
                let Some(obj) = item.as_object() else {
                    continue;
                };
                if obj.get("enabled").and_then(Value::as_bool) == Some(false) {
                    continue;
                }
                let key = obj.get("key").and_then(Value::as_str).unwrap_or("").trim();
                if key.is_empty() {
                    continue;
                }
                let val = obj
                    .get("value")
                    .and_then(env_value_to_string)
                    .unwrap_or_default();
                out.insert(key.to_string(), val);
            }
        }
        Value::Object(obj) => {
            for (k, v) in obj {
                if k.is_empty() {
                    continue;
                }
                if let Some(s) = env_value_to_string(v) {
                    out.insert(k.clone(), s);
                }
            }
        }
        _ => {}
    }
    out
}

pub(crate) fn strip_workbench_env_from_config(config: &mut Value) {
    if let Some(obj) = config.as_object_mut() {
        obj.remove("workbench_env");
    }
}

pub const GATE_REASON_NOT_RUN: &str = "not_run";
pub const GATE_REASON_SPEC_CHANGED: &str = "spec_changed";
pub const GATE_REASON_UNRESOLVED: &str = "unresolved_vars";
pub const GATE_REASON_SKIPPED: &str = "skipped";

#[derive(Debug, Clone)]
pub struct RecentGateRun {
    pub endpoint_key: Option<String>,
    pub spec_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DraftGateEval {
    pub items: Vec<DraftGateItem>,
    pub leftover: Vec<String>,
}

pub fn evaluate_endpoint_gates(
    endpoints: &Value,
    env: &HashMap<String, String>,
    supplier_cfg: Option<&SupplierConfig>,
    upstream_url: &str,
    recent: &[RecentGateRun],
) -> DraftGateEval {
    let mut leftover: Vec<String> = Vec::new();
    let mut leftover_seen: HashSet<String> = HashSet::new();
    let Some(arr) = endpoints.as_array().filter(|a| !a.is_empty()) else {
        if let Err(e) =
            build_workbench_spec(&serde_json::json!({}), env, supplier_cfg, upstream_url)
        {
            merge_leftover(&e, &mut leftover, &mut leftover_seen);
        }
        return DraftGateEval {
            items: Vec::new(),
            leftover,
        };
    };
    let mut items = Vec::with_capacity(arr.len());
    for (i, ep) in arr.iter().enumerate() {
        let fallback = format!("ep-{}", i + 1);
        let key = workbench_endpoint_key(ep, Some(&fallback)).unwrap_or_else(|| fallback.clone());
        let gated = ep.get("gate").and_then(Value::as_bool).unwrap_or(true);
        let built = build_workbench_spec(ep, env, supplier_cfg, upstream_url);
        let (fingerprint, unresolved) = match &built {
            Ok(b) => (Some(b.fingerprint.clone()), false),
            Err(e) => {
                let u = merge_leftover(e, &mut leftover, &mut leftover_seen);
                (None, u)
            }
        };
        if !gated {
            items.push(DraftGateItem {
                endpoint_key: key,
                fingerprint,
                met: true,
                reason: GATE_REASON_SKIPPED.to_string(),
            });
            continue;
        }
        if unresolved {
            items.push(DraftGateItem {
                endpoint_key: key,
                fingerprint: None,
                met: false,
                reason: GATE_REASON_UNRESOLVED.to_string(),
            });
            continue;
        }
        let Some(fp) = fingerprint else {
            items.push(DraftGateItem {
                endpoint_key: key,
                fingerprint: None,
                met: false,
                reason: GATE_REASON_NOT_RUN.to_string(),
            });
            continue;
        };
        let aliases = endpoint_key_aliases(ep, &key);
        let matching: Vec<&RecentGateRun> = recent
            .iter()
            .filter(|r| {
                r.endpoint_key
                    .as_deref()
                    .is_some_and(|k| aliases.iter().any(|a| a == k))
            })
            .collect();
        let met = matching
            .iter()
            .any(|r| r.spec_fingerprint.as_deref() == Some(fp.as_str()));
        let reason = if met {
            String::new()
        } else if matching.is_empty() {
            GATE_REASON_NOT_RUN.to_string()
        } else {
            GATE_REASON_SPEC_CHANGED.to_string()
        };
        items.push(DraftGateItem {
            endpoint_key: key,
            fingerprint: Some(fp),
            met,
            reason,
        });
    }
    DraftGateEval { items, leftover }
}

fn endpoint_key_aliases(ep: &Value, primary: &str) -> Vec<String> {
    let mut out = vec![primary.to_string()];
    for field in ["id", "name", "path"] {
        if let Some(s) = ep
            .get(field)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            if !out.iter().any(|x| x == s) {
                out.push(s.to_string());
            }
        }
    }
    out
}

fn merge_leftover(err: &ApiError, leftover: &mut Vec<String>, seen: &mut HashSet<String>) -> bool {
    let s = err.to_string();
    let Some(idx) = s.find("UNRESOLVED_VARS:") else {
        return false;
    };
    let rest = s[idx + "UNRESOLVED_VARS:".len()..].trim();
    for name in rest.split(',') {
        let name = name.trim();
        if !name.is_empty() && seen.insert(name.to_string()) {
            leftover.push(name.to_string());
        }
    }
    true
}

pub(crate) fn workbench_endpoint_key(ep: &Value, fallback: Option<&str>) -> Option<String> {
    ep.get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            ep.get("name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            ep.get("path")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .or_else(|| fallback.filter(|s| !s.is_empty()).map(str::to_string))
}

pub(crate) fn apply_env_to_supplier_config(
    cfg: &mut SupplierConfig,
    env: &HashMap<String, String>,
) {
    if let Some(ref mut flow) = cfg.auth_flow {
        expand_auth_flow(flow, env);
    }
}

pub(crate) fn apply_env_str(input: &str, env: &HashMap<String, String>) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            let mut j = i + 2;
            while j < bytes.len() && is_ascii_ws(bytes[j]) {
                j += 1;
            }
            let name_start = j;
            while j < bytes.len() && is_ident_byte(bytes[j]) {
                j += 1;
            }
            let name_end = j;
            while j < bytes.len() && is_ascii_ws(bytes[j]) {
                j += 1;
            }
            if name_end > name_start
                && j + 1 < bytes.len()
                && bytes[j] == b'}'
                && bytes[j + 1] == b'}'
            {
                let name = &input[name_start..name_end];
                if let Some(val) = env.get(name) {
                    out.push_str(val);
                } else {
                    out.push_str("{{");
                    out.push_str(name);
                    out.push_str("}}");
                }
                i = j + 2;
                continue;
            }
        }
        let ch = input[i..].chars().next().unwrap_or('\u{fffd}');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

pub(crate) fn leftover_scan_json(
    upstream_url: &str,
    cfg: Option<&SupplierConfig>,
    spec: &ChannelEgressSpec,
    payload: &Value,
) -> Value {
    let auth_flow = cfg
        .and_then(|c| c.auth_flow.as_ref())
        .map(auth_flow_scan_value);
    let spec_v = serde_json::to_value(spec).unwrap_or(Value::Null);
    serde_json::json!({
        "upstream_url": upstream_url,
        "auth_flow": auth_flow,
        "spec": spec_v,
        "payload": payload,
    })
}

pub(crate) fn unresolved_idents_in(value: &Value) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    walk_unresolved(value, &mut names, &mut seen);
    names
}

pub(crate) fn fingerprint_workbench(
    upstream_url: &str,
    spec: &ChannelEgressSpec,
    payload: &Value,
    cred_keys: &[String],
) -> Result<String, ApiError> {
    let spec_v = serde_json::to_value(spec)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    let obj = serde_json::json!({
        "upstream_url": upstream_url,
        "spec": spec_v,
        "payload": payload,
        "cred_keys": cred_keys,
    });
    Ok(sha256_canonical(&obj))
}

fn sha256_canonical(value: &Value) -> String {
    let canon = canonicalize(value);
    let json = serde_json::to_string(&canon).unwrap_or_else(|_| "null".into());
    let digest = Sha256::digest(json.as_bytes());
    hex::encode(digest)
}

fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                if let Some(v) = map.get(k) {
                    out.insert(k.clone(), canonicalize(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(canonicalize).collect()),
        other => other.clone(),
    }
}

/// 端点字段中的非空字符串。
fn non_empty_str<'a>(ep: &'a Value, key: &str) -> Option<&'a str> {
    ep.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}

/// 端点必填字符串字段。
fn required_str<'a>(ep: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    non_empty_str(ep, key)
        .ok_or_else(|| ApiError::validation(format!("CONFIG_INVALID: endpoint.{key} 必填")))
}

/// 出口规格的 `request` 段（`method`、`path`、`body_encoding` 必填；`headers`、`query` 等可选）。
fn request_json_from_endpoint(ep: &Value) -> Result<serde_json::Map<String, Value>, ApiError> {
    let mut req = serde_json::Map::new();
    for key in ["method", "path", "body_encoding"] {
        req.insert(key.into(), Value::String(required_str(ep, key)?.into()));
    }
    let headers = kv_from_json(ep.get("headers"));
    if !headers.is_empty() {
        req.insert("headers".into(), json_from_kv(&headers));
    }
    let query = kv_from_json(ep.get("query"));
    if !query.is_empty() {
        req.insert("query".into(), json_from_kv(&query));
    }
    if let Some(wrap) = non_empty_str(ep, "wrap") {
        req.insert("wrap".into(), Value::String(wrap.into()));
    }
    if let Some(ct) = ep
        .get("content_type")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        req.insert("content_type".into(), Value::String(ct.into()));
    }
    Ok(req)
}

/// 从端点 JSON 组装出口规格的 JSON 形态（尚未反序列化）。
/// 端点未单独指定 `base_url` 时写入供应商地址，运行时不再回退。
fn spec_json_from_endpoint(ep: &Value, upstream_url: &str) -> Result<Value, ApiError> {
    let mut obj = serde_json::Map::new();
    let adapter = non_empty_str(ep, "adapter").unwrap_or(ADAPTER_HTTP_JSON);
    obj.insert("adapter".into(), Value::String(adapter.into()));
    obj.insert(
        "request".into(),
        Value::Object(request_json_from_endpoint(ep)?),
    );
    let base_url = non_empty_str(ep, "base_url").unwrap_or(upstream_url);
    obj.insert("base_url".into(), Value::String(base_url.into()));
    for key in ["operation", "artifact_id"] {
        if let Some(v) = non_empty_str(ep, key) {
            obj.insert(key.into(), Value::String(v.into()));
        }
    }
    if let Some(unwrap) = non_empty_str(ep, "unwrap") {
        obj.insert("response".into(), serde_json::json!({ "unwrap": unwrap }));
    }
    for key in ["graphql", "job_poll", "sign_form", "soap", "ws", "auth"] {
        if let Some(v) = ep.get(key) {
            obj.insert(key.into(), v.clone());
        }
    }
    if let Some(ms) = ep.get("timeout_ms").and_then(serde_json::Value::as_u64) {
        obj.insert("timeout_ms".into(), serde_json::json!(ms));
    }
    Ok(Value::Object(obj))
}

fn assemble_spec(
    ep: &Value,
    upstream_url: &str,
    supplier: Option<&SupplierConfig>,
) -> Result<ChannelEgressSpec, ApiError> {
    let spec = spec_json_from_endpoint(ep, upstream_url)?;
    let mut spec: ChannelEgressSpec = serde_json::from_value(spec)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    let ep_profile = spec.auth.as_ref().map_or("", |a| a.profile.as_str());
    if inherit_requires_session_profile(supplier, Some(ep_profile)) {
        return Err(ApiError::validation("请设置会话鉴权类型".to_string()));
    }
    if let Some(cfg) = supplier {
        merge_auth_from_supplier(&mut spec, cfg);
    }
    mazbri_core::sanitize_egress_auth(&mut spec);
    strip_inject_key(&mut spec, supplier);
    validate_egress_spec(&spec)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    Ok(spec)
}

fn strip_inject_key(spec: &mut ChannelEgressSpec, supplier: Option<&SupplierConfig>) {
    let profile = spec
        .auth
        .as_ref()
        .and_then(|a| mazbri_core::AuthProfile::parse(&a.profile).ok());
    let flow = supplier
        .and_then(|c| c.auth_flow.as_ref())
        .filter(|f| f.enabled);
    let inject_key = spec
        .auth
        .as_ref()
        .and_then(|a| a.inject.as_ref())
        .map(|i| i.key.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            flow.map(|f| {
                if f.inject_key.trim().is_empty() {
                    "Authorization".to_string()
                } else {
                    f.inject_key.trim().to_string()
                }
            })
        })
        .unwrap_or_else(|| "Authorization".into());
    let inject_to = spec
        .auth
        .as_ref()
        .and_then(|a| a.inject.as_ref())
        .map(|i| i.to.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| flow.map(|f| f.inject_to.trim().to_string()))
        .unwrap_or_else(|| "header".into());
    let req = &mut spec.request;
    if let Some(ref mut headers) = req.headers {
        headers.retain(|k, _| {
            let kl = k.to_ascii_lowercase();
            if kl == "authorization" {
                return false;
            }
            if kl == inject_key.to_ascii_lowercase() {
                return false;
            }
            if matches!(profile, Some(mazbri_core::AuthProfile::AwsSigV4))
                && (kl == "x-amz-date"
                    || kl == "x-amz-security-token"
                    || kl == "x-amz-content-sha256")
            {
                return false;
            }
            true
        });
        if headers.is_empty() {
            req.headers = None;
        }
    }
    if inject_to.eq_ignore_ascii_case("query") {
        if let Some(ref mut query) = req.query {
            query.retain(|k, _| !k.eq_ignore_ascii_case(&inject_key));
            if query.is_empty() {
                req.query = None;
            }
        }
    }
}

/// L1 tenant JSON. `body_encoding=none` → Null; otherwise the endpoint body (missing = `{}`).
fn payload_from_endpoint(ep: &Value) -> Value {
    if non_empty_str(ep, "body_encoding") == Some("none") {
        return Value::Null;
    }
    match ep.get("body") {
        None | Some(Value::Null) => serde_json::json!({}),
        Some(body) => body.clone(),
    }
}

fn expand_endpoint(ep: &Value, env: &HashMap<String, String>) -> Result<Value, ApiError> {
    let mut ep = match ep {
        Value::Object(_) => ep.clone(),
        _ => serde_json::json!({}),
    };
    let obj = ep
        .as_object_mut()
        .ok_or_else(|| ApiError::validation("endpoint must be object"))?;
    for key in [
        "path",
        "method",
        "operation",
        "wrap",
        "unwrap",
        "base_url",
        "content_type",
        "body_encoding",
    ] {
        expand_string_field(obj, key, env);
    }
    for key in ["headers", "query"] {
        if let Some(v) = obj.get(key).cloned() {
            obj.insert(
                key.to_string(),
                json_from_kv(&expand_kv_map(&kv_from_json(Some(&v)), env)),
            );
        }
    }
    if let Some(body) = obj.get("body").cloned() {
        obj.insert("body".into(), expand_json_body(&body, env)?);
    }
    if let Some(gql) = obj.get("graphql").cloned() {
        obj.insert("graphql".into(), expand_graphql(&gql, env));
    }
    if let Some(jp) = obj.get("job_poll").cloned() {
        obj.insert("job_poll".into(), expand_job_poll(&jp, env));
    }
    if let Some(ws) = obj.get("ws").cloned() {
        obj.insert("ws".into(), expand_ws(&ws, env));
    }
    if let Some(sf) = obj.get("sign_form").cloned() {
        obj.insert("sign_form".into(), expand_string_tree(&sf, env));
    }
    if let Some(soap) = obj.get("soap").cloned() {
        obj.insert("soap".into(), expand_string_tree(&soap, env));
    }
    Ok(ep)
}

fn expand_graphql(gql: &Value, env: &HashMap<String, String>) -> Value {
    let mut gql = gql.clone();
    let Some(obj) = gql.as_object_mut() else {
        return gql;
    };
    expand_string_field(obj, "document", env);
    expand_string_field(obj, "operation_name", env);
    for key in ["variables_map", "enum_map"] {
        if let Some(v) = obj.get(key).cloned() {
            obj.insert(
                key.to_string(),
                json_from_kv(&expand_kv_map(&kv_from_json(Some(&v)), env)),
            );
        }
    }
    gql
}

fn expand_job_poll(jp: &Value, env: &HashMap<String, String>) -> Value {
    let mut jp = jp.clone();
    let Some(obj) = jp.as_object_mut() else {
        return jp;
    };
    expand_string_field(obj, "submit_path", env);
    expand_string_field(obj, "query_path", env);
    jp
}

fn expand_ws(ws: &Value, env: &HashMap<String, String>) -> Value {
    let mut ws = ws.clone();
    let Some(obj) = ws.as_object_mut() else {
        return ws;
    };
    expand_string_field(obj, "path", env);
    ws
}

fn expand_string_tree(v: &Value, env: &HashMap<String, String>) -> Value {
    match v {
        Value::String(s) => Value::String(apply_env_str(s, env)),
        Value::Array(arr) => Value::Array(arr.iter().map(|x| expand_string_tree(x, env)).collect()),
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, val) in map {
                let nk = apply_env_str(k, env);
                out.insert(nk, expand_string_tree(val, env));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn expand_json_body(body: &Value, env: &HashMap<String, String>) -> Result<Value, ApiError> {
    if body.is_null() {
        return Ok(Value::Null);
    }
    let raw = serde_json::to_string(body)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    let expanded = apply_env_str(&raw, env);
    serde_json::from_str(&expanded)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))
}

fn expand_string_field(obj: &mut Map<String, Value>, key: &str, env: &HashMap<String, String>) {
    if let Some(Value::String(s)) = obj.get(key) {
        let n = apply_env_str(s, env);
        obj.insert(key.to_string(), Value::String(n));
    }
}

fn expand_auth_flow(flow: &mut AuthFlowConfig, env: &HashMap<String, String>) {
    flow.token_url = apply_env_str(&flow.token_url, env);
    flow.method = apply_env_str(&flow.method, env);
    flow.inject_to = apply_env_str(&flow.inject_to, env);
    flow.inject_key = apply_env_str(&flow.inject_key, env);
    flow.inject_value_format = apply_env_str(&flow.inject_value_format, env);
    flow.request_body_tpl = apply_env_str(&flow.request_body_tpl, env);
    flow.headers = expand_kv_map(&flow.headers, env);
}

fn expand_kv_map(
    map: &HashMap<String, String>,
    env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (k, v) in map {
        let nk = apply_env_str(k, env);
        if nk.is_empty() {
            continue;
        }
        out.insert(nk, apply_env_str(v, env));
    }
    out
}

fn kv_from_json(v: Option<&Value>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(v) = v else {
        return out;
    };
    match v {
        Value::Object(map) => {
            for (k, val) in map {
                if k.is_empty() {
                    continue;
                }
                out.insert(k.clone(), json_scalar_string(val));
            }
        }
        Value::Array(arr) => {
            for item in arr {
                let enabled = item
                    .get("enabled")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true);
                if !enabled {
                    continue;
                }
                let k = item
                    .get("key")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .trim();
                if k.is_empty() {
                    continue;
                }
                let val = item
                    .get("value")
                    .map(json_scalar_string)
                    .unwrap_or_default();
                out.insert(k.to_string(), val);
            }
        }
        _ => {}
    }
    out
}

fn json_from_kv(map: &HashMap<String, String>) -> Value {
    let mut obj = Map::new();
    for (k, v) in map {
        if k.is_empty() {
            continue;
        }
        obj.insert(k.clone(), Value::String(v.clone()));
    }
    Value::Object(obj)
}

fn json_scalar_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

fn cred_keys_of(cfg: Option<&SupplierConfig>) -> Vec<String> {
    let mut keys: Vec<String> = cfg
        .and_then(|c| c.auth_flow.as_ref())
        .map(|f| f.credentials.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn auth_flow_scan_value(flow: &AuthFlowConfig) -> Value {
    serde_json::json!({
        "token_url": flow.token_url,
        "method": flow.method,
        "inject_to": flow.inject_to,
        "inject_key": flow.inject_key,
        "inject_value_format": flow.inject_value_format,
        "request_body_tpl": flow.request_body_tpl,
        "headers": flow.headers,
    })
}

fn walk_unresolved(
    value: &Value,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    match value {
        Value::String(s) => collect_unresolved_from_str(s, names, seen),
        Value::Array(arr) => {
            for v in arr {
                walk_unresolved(v, names, seen);
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                collect_unresolved_from_str(k, names, seen);
                walk_unresolved(v, names, seen);
            }
        }
        _ => {}
    }
}

fn collect_unresolved_from_str(
    input: &str,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            let mut j = i + 2;
            while j < bytes.len() && is_ascii_ws(bytes[j]) {
                j += 1;
            }
            let name_start = j;
            while j < bytes.len() && is_ident_byte(bytes[j]) {
                j += 1;
            }
            let name_end = j;
            while j < bytes.len() && is_ascii_ws(bytes[j]) {
                j += 1;
            }
            if name_end > name_start
                && j + 1 < bytes.len()
                && bytes[j] == b'}'
                && bytes[j + 1] == b'}'
            {
                let name = &input[name_start..name_end];
                if !EXECUTE_PREFIXES.iter().any(|p| name.starts_with(p))
                    && seen.insert(name.to_string())
                {
                    names.push(name.to_string());
                }
                i = j + 2;
                continue;
            }
        }
        let ch = input[i..].chars().next().unwrap_or('\u{fffd}');
        i += ch.len_utf8();
    }
}

fn is_ascii_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-'
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_core::AuthFlowConfig;

    fn empty_env() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn ignores_ep_egress_copies_tab_headers() {
        let ep = serde_json::json!({
            "method": "GET",
            "path": "/v1", "body_encoding": "json",
            "headers": { "X-Version": "1" },
            "egress": {
                "adapter": "http_json",
                "request": { "method": "POST", "path": "/old" }
            }
        });
        let built =
            build_workbench_spec(&ep, &empty_env(), None, "https://up.example").expect("build");
        let req = built.spec.request;
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/v1");
        let headers = req.headers.expect("headers");
        assert_eq!(headers.get("X-Version").map(String::as_str), Some("1"));
    }

    #[test]
    fn payload_comes_from_endpoint_body() {
        let ep = serde_json::json!({
            "method": "POST",
            "path": "/", "body_encoding": "json",
            "body": { "from": "db" }
        });
        let built =
            build_workbench_spec(&ep, &empty_env(), None, "https://up.example").expect("build");
        assert_eq!(built.payload["from"], "db");
        assert!(built.spec.request.headers.is_none(),);
    }

    #[test]
    fn expands_env_in_query_and_body() {
        let ep = serde_json::json!({
            "method": "POST",
            "path": "/x", "body_encoding": "json",
            "query": { "appId": "{{appId}}" },
            "body": { "appId": "{{appId}}" }
        });
        let mut env = HashMap::new();
        env.insert("appId".into(), "1".into());
        let built = build_workbench_spec(&ep, &env, None, "https://up.example").expect("build");
        let q = built.spec.request.query.as_ref().unwrap();
        assert_eq!(q.get("appId").map(String::as_str), Some("1"));
        assert_eq!(built.payload["appId"], "1");
        let leftover = unresolved_idents_in(&leftover_scan_json(
            &built.spec.base_url,
            None,
            &built.spec,
            &built.payload,
        ));
        assert_eq!(leftover, [] as [std::string::String; 0]);
    }

    fn err_contains_unresolved(err: &ApiError, name: &str) {
        let s = err.to_string();
        assert!(
            s.contains("UNRESOLVED_VARS") && s.contains(name),
            "expected UNRESOLVED_VARS with {name}, got {s}"
        );
    }

    #[test]
    fn leftover_ident_fails_pr2() {
        let ep = serde_json::json!({
            "method": "GET",
            "path": "/p/{{missing}}", "body_encoding": "json",
        });
        let err = build_workbench_spec(&ep, &empty_env(), None, "https://up.example")
            .expect_err("pr2 leftover must 4xx");
        err_contains_unresolved(&err, "missing");
    }

    #[test]
    fn leftover_in_token_url_fails() {
        let ep = serde_json::json!({ "method": "GET", "path": "/", "body_encoding": "json" });
        let cfg = SupplierConfig {
            auth_flow: Some(AuthFlowConfig {
                enabled: true,
                profile: Some("oauth_cc_form".into()),
                token_url: "https://idp.example/{{missing}}".into(),
                inject_key: "Authorization".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = build_workbench_spec(&ep, &empty_env(), Some(&cfg), "https://up.example")
            .expect_err("token_url leftover");
        err_contains_unresolved(&err, "missing");
    }

    #[test]
    fn leftover_in_graphql_map_fails() {
        let ep = serde_json::json!({
            "method": "POST",
            "path": "/", "body_encoding": "json",
            "adapter": "graphql",
            "operation": "Q",
            "graphql": {
                "document": "query Q { x }",
                "variables_map": { "id": "{{missing}}" }
            }
        });
        let err = build_workbench_spec(&ep, &empty_env(), None, "https://up.example")
            .expect_err("graphql leftover");
        err_contains_unresolved(&err, "missing");
    }

    #[test]
    fn strips_inject_authorization_keeps_static_header() {
        let ep = serde_json::json!({
            "method": "GET",
            "path": "/", "body_encoding": "json",
            "headers": {
                "Authorization": "Bearer hand",
                "X-Version": "1"
            }
        });
        let cfg = SupplierConfig {
            auth_flow: Some(AuthFlowConfig {
                enabled: true,
                profile: Some("oauth_cc_form".into()),
                token_url: "https://idp.example/token".into(),
                inject_key: "Authorization".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let built = build_workbench_spec(&ep, &empty_env(), Some(&cfg), "https://up.example")
            .expect("build");
        let headers = built.spec.request.headers.clone().unwrap_or_default();
        assert!(
            !headers
                .keys()
                .any(|k| k.eq_ignore_ascii_case("authorization")),
            "inject_key must be stripped, got {headers:?}"
        );
        assert_eq!(headers.get("X-Version").map(String::as_str), Some("1"));
    }

    #[test]
    fn empty_env_value_counts_as_hit() {
        let mut env = HashMap::new();
        env.insert("appId".into(), String::new());
        assert_eq!(apply_env_str("id={{appId}}", &env), "id=");
    }

    #[test]
    fn execute_placeholders_are_not_unresolved() {
        let v = serde_json::json!({
            "tpl": "{{request.body}} {{credential.secret}} {{missing}}"
        });
        let leftover = unresolved_idents_in(&v);
        assert_eq!(leftover, vec!["missing".to_string()]);
    }

    #[test]
    fn fingerprint_includes_base_url_and_is_stable() {
        let ep = serde_json::json!({
            "method": "GET",
            "path": "/ping", "body_encoding": "json"
        });
        let a = build_workbench_spec(&ep, &empty_env(), None, "https://a.example").expect("a");
        let b = build_workbench_spec(&ep, &empty_env(), None, "https://b.example").expect("b");
        assert_ne!(a.fingerprint, b.fingerprint);
        let a2 = build_workbench_spec(&ep, &empty_env(), None, "https://a.example").expect("a2");
        assert_eq!(a.fingerprint, a2.fingerprint);
    }

    #[test]
    fn fingerprint_golden_hex() {
        let spec = ChannelEgressSpec {
            adapter: ADAPTER_HTTP_JSON.into(),
            base_url: "https://api.example.com".into(),
            request: mazbri_core::EgressRequestSpec {
                method: "GET".into(),
                path: "/ping".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let hex = fingerprint_workbench(
            "https://api.example.com",
            &spec,
            &serde_json::json!({}),
            &[],
        )
        .expect("fp");
        assert_eq!(
            hex,
            "6eed78d1119b5597ebc8ab612d8b2cbfe4324f0dbedbf9d712392bab75ed43f5"
        );
    }

    #[test]
    fn gate_skipped_not_run_spec_changed_and_met() {
        let endpoints = serde_json::json!([
            { "id": "ep-skip", "path": "/s", "body_encoding": "json", "method": "GET", "gate": false },
            { "id": "ep-a", "path": "/a", "body_encoding": "json", "method": "GET" },
            { "id": "ep-b", "path": "/b", "body_encoding": "json", "method": "GET" },
            { "id": "ep-c", "path": "/c", "body_encoding": "json", "method": "GET" }
        ]);
        let env = empty_env();
        let fp_b = build_workbench_spec(&endpoints[2], &env, None, "https://up.example")
            .expect("b")
            .fingerprint;
        let fp_c_old = "deadbeef".to_string();
        let recent = vec![
            RecentGateRun {
                endpoint_key: Some("ep-b".into()),
                spec_fingerprint: Some(fp_b),
            },
            RecentGateRun {
                endpoint_key: Some("ep-c".into()),
                spec_fingerprint: Some(fp_c_old),
            },
        ];
        let eval = evaluate_endpoint_gates(&endpoints, &env, None, "https://up.example", &recent);
        assert_eq!(eval.leftover, [] as [std::string::String; 0]);
        assert_eq!(eval.items[0].reason, GATE_REASON_SKIPPED);
        assert!(eval.items[0].met);
        assert_eq!(eval.items[1].reason, GATE_REASON_NOT_RUN);
        assert!(!eval.items[1].met);
        assert!(eval.items[2].met);
        assert_eq!(eval.items[3].reason, GATE_REASON_SPEC_CHANGED);
        assert!(!eval.items[3].met);
    }

    #[test]
    fn gate_unresolved_and_auth_only_does_not_count() {
        let endpoints = serde_json::json!([
            { "id": "ep-a", "path": "/{{missing}}", "body_encoding": "json", "method": "GET" }
        ]);
        let eval = evaluate_endpoint_gates(
            &endpoints,
            &empty_env(),
            None,
            "https://up.example",
            &[RecentGateRun {
                endpoint_key: Some("ep-a".into()),
                spec_fingerprint: Some("anything".into()),
            }],
        );
        assert_eq!(eval.leftover, vec!["missing".to_string()]);
        assert_eq!(eval.items[0].reason, GATE_REASON_UNRESOLVED);
        assert!(!eval.items[0].met);
    }

    #[test]
    fn post_none_payload_is_null_and_encoding_none() {
        let ep = serde_json::json!({
            "method": "POST",
            "path": "/", "body_encoding": "none"
        });
        let built =
            build_workbench_spec(&ep, &empty_env(), None, "https://up.example").expect("build");
        assert!(built.payload.is_null());
        assert_eq!(built.spec.request.body_encoding.as_str(), "none");
    }

    #[test]
    fn post_json_empty_object_stays_object() {
        let ep = serde_json::json!({
            "method": "POST",
            "path": "/", "body_encoding": "json",
            "body_content_type": "json",
            "body": {}
        });
        let built =
            build_workbench_spec(&ep, &empty_env(), None, "https://up.example").expect("build");
        assert_eq!(built.payload, serde_json::json!({}));
        assert_eq!(
            built.spec.request.body_encoding,
            mazbri_core::BodyEncoding::Json
        );
    }

    #[test]
    fn missing_body_encoding_is_rejected() {
        let ep = serde_json::json!({ "method": "POST", "path": "/" });
        let err = build_workbench_spec(&ep, &empty_env(), None, "https://up.example")
            .expect_err("must reject");
        assert!(err.to_string().contains("body_encoding"));
    }

    #[test]
    fn missing_path_is_rejected() {
        let ep = serde_json::json!({ "method": "GET", "body_encoding": "json" });
        let err = build_workbench_spec(&ep, &empty_env(), None, "https://up.example")
            .expect_err("must reject");
        assert!(err.to_string().contains("path"));
    }

    #[test]
    fn none_and_empty_object_have_different_fingerprints() {
        let none_ep = serde_json::json!({
            "method": "POST",
            "path": "/", "body_encoding": "none"
        });
        let empty_ep = serde_json::json!({
            "method": "POST",
            "path": "/", "body_encoding": "json",
            "body": {}
        });
        let a =
            build_workbench_spec(&none_ep, &empty_env(), None, "https://up.example").expect("none");
        let b = build_workbench_spec(&empty_ep, &empty_env(), None, "https://up.example")
            .expect("empty");
        assert_ne!(a.fingerprint, b.fingerprint);
    }

    #[test]
    fn content_type_override_enters_spec_and_fingerprint() {
        let ep = serde_json::json!({
            "method": "POST",
            "path": "/", "body_encoding": "json",
            "body": {"a": 1},
            "content_type": "application/vnd.api+json"
        });
        let built =
            build_workbench_spec(&ep, &empty_env(), None, "https://up.example").expect("build");
        assert_eq!(
            built.spec.request.content_type.as_deref(),
            Some("application/vnd.api+json")
        );
        let plain = serde_json::json!({
            "method": "POST",
            "path": "/", "body_encoding": "json",
            "body": {"a": 1}
        });
        let other =
            build_workbench_spec(&plain, &empty_env(), None, "https://up.example").expect("plain");
        assert_ne!(built.fingerprint, other.fingerprint);
    }

    #[test]
    fn form_encoding_enters_spec() {
        let ep = serde_json::json!({
            "method": "POST",
            "path": "/",
            "body": {"a": 1, "b": "x"},
            "body_encoding": "form"
        });
        let built =
            build_workbench_spec(&ep, &empty_env(), None, "https://up.example").expect("build");
        assert_eq!(built.spec.request.body_encoding.as_str(), "form");
        assert_eq!(built.payload["a"], 1);
    }

    #[test]
    fn non_object_body_is_not_wrapped_in_value() {
        let ep = serde_json::json!({
            "method": "POST",
            "path": "/", "body_encoding": "json",
            "body": [1, 2]
        });
        let built =
            build_workbench_spec(&ep, &empty_env(), None, "https://up.example").expect("build");
        assert_eq!(built.payload, serde_json::json!([1, 2]));
    }

    #[test]
    fn inherit_without_session_profile_is_rejected() {
        let ep = serde_json::json!({ "method": "GET", "path": "/", "body_encoding": "json" });
        let cfg = SupplierConfig {
            auth_flow: Some(AuthFlowConfig {
                enabled: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = build_workbench_spec(&ep, &empty_env(), Some(&cfg), "https://up.example")
            .expect_err("inherit without profile");
        assert!(err.to_string().contains("请设置会话鉴权类型"), "{err}");
    }

    #[test]
    fn inherit_basic_strips_session_token_url() {
        let ep = serde_json::json!({
            "method": "GET",
            "path": "/", "body_encoding": "json",
            "auth": { "profile": "none" }
        });
        let cfg = SupplierConfig {
            auth_flow: Some(AuthFlowConfig {
                enabled: true,
                profile: Some("basic".into()),
                token_url: "https://idp.example/token".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let built = build_workbench_spec(&ep, &empty_env(), Some(&cfg), "https://up.example")
            .expect("build");
        let auth = built.spec.auth.expect("auth");
        assert_eq!(auth.profile, "basic");
        assert!(auth.token_url.is_none());
    }

    #[test]
    fn basic_endpoint_sanitizes_token_url() {
        let ep = serde_json::json!({
            "method": "GET",
            "path": "/", "body_encoding": "json",
            "auth": {
                "profile": "basic",
                "token_url": "https://idp.example/token"
            }
        });
        let built =
            build_workbench_spec(&ep, &empty_env(), None, "https://up.example").expect("build");
        let auth = built.spec.auth.expect("auth");
        assert_eq!(auth.profile, "basic");
        assert!(auth.token_url.is_none());
    }
}
