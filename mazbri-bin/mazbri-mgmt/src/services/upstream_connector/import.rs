use super::{
    ApiError, Arc, CreateDraftRequest, CurlImportRequest, Decimal, DraftDto, Duration, ExecuteMode,
    ExecuteOptions, HashMap, InvokeInput, OpenApiImportRequest, PgPool, PostmanImportRequest, Row,
    RunRequest, RuntimeEngine, SupplierConfig, Uuid, build_workbench_spec, config_json_for_draft,
    create_draft, endpoint_by_id, get_draft, mask_config_credentials, parse_workbench_env, repo,
    with_gate, workbench_endpoint_key,
};
use mazbri_core::{
    ADAPTER_HTTP_JSON, BodyEncoding, ChannelEgressSpec, parse_channel_config_egress,
    profile_is_undeclared,
};

// ---------- Import ----------

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn import_curl(
    pool: &PgPool,
    req: CurlImportRequest,
    actor: &str,
) -> Result<DraftDto, ApiError> {
    let parsed = parse_curl(&req.curl)?;
    let draft_code = req
        .draft_code
        .unwrap_or_else(|| format!("curl-{}", &Uuid::new_v4().to_string()[..8]));
    let name = req.name.unwrap_or_else(|| "Imported from cURL".into());
    let endpoint = serde_json::json!([{
        "id": "imported",
        "name": format!("{} {}", parsed.method, parsed.path),
        "method": parsed.method,
        "path": parsed.path,
        "headers": parsed.headers,
        "body": parsed.body,
        "query": parsed.query,
        "assertions": [{"type":"status_range","min":200,"max":399}],
        "gate": true
    }]);
    create_draft(
        pool,
        CreateDraftRequest {
            draft_code,
            name,
            description: Some("Imported from cURL".into()),
            upstream_type: "transparent".into(),
            upstream_url: parsed.base_url,
            config: Some(serde_json::json!({"upstream_type":"transparent"})),
            endpoints: Some(endpoint),
            workbench_env: None,
            source: Some("curl".into()),
        },
        actor,
    )
    .await
}

pub(super) struct ParsedCurl {
    method: String,
    base_url: String,
    path: String,
    headers: HashMap<String, String>,
    query: HashMap<String, String>,
    body: Option<serde_json::Value>,
}

pub(super) fn parse_curl(curl: &str) -> Result<ParsedCurl, ApiError> {
    let mut method = "GET".to_string();
    let mut url = String::new();
    let mut headers = HashMap::new();
    let mut body: Option<String> = None;

    let tokens = shell_split(curl);
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        if t == "curl" {
            i += 1;
            continue;
        }
        if (t == "-X" || t == "--request")
            && let Some(m) = tokens.get(i + 1)
        {
            method = m.to_uppercase();
            i += 2;
            continue;
        }
        if (t == "-H" || t == "--header")
            && let Some(h) = tokens.get(i + 1)
        {
            if let Some((k, v)) = h.split_once(':') {
                headers.insert(k.trim().to_string(), v.trim().to_string());
            }
            i += 2;
            continue;
        }
        if (t == "-d" || t == "--data" || t == "--data-raw" || t == "--data-binary")
            && let Some(d) = tokens.get(i + 1)
        {
            body = Some(d.clone());
            if method == "GET" {
                method = "POST".into();
            }
            i += 2;
            continue;
        }
        if !t.starts_with('-') && (t.starts_with("http://") || t.starts_with("https://")) {
            url = t.trim_matches('\'').trim_matches('"').to_string();
        }
        i += 1;
    }

    if url.is_empty() {
        return Err(ApiError::validation("无法从 cURL 解析 URL"));
    }

    let parsed =
        url::Url::parse(&url).map_err(|e| ApiError::validation(format!("URL 无效: {e}")))?;
    let base_url = format!(
        "{}://{}{}",
        parsed.scheme(),
        parsed.host_str().unwrap_or(""),
        parsed.port().map(|p| format!(":{p}")).unwrap_or_default()
    );
    let path = if parsed.path().is_empty() {
        "/".into()
    } else {
        parsed.path().to_string()
    };
    let mut query = HashMap::new();
    for (k, v) in parsed.query_pairs() {
        query.insert(k.to_string(), v.to_string());
    }
    let body_json = body.map(|b| {
        serde_json::from_str::<serde_json::Value>(&b).unwrap_or(serde_json::Value::String(b))
    });

    Ok(ParsedCurl {
        method,
        base_url,
        path,
        headers,
        query,
        body: body_json,
    })
}

pub(super) fn shell_split(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ' ' | '\t' | '\n' if !in_single && !in_double => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            '\\' if !in_single => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn import_postman(
    pool: &PgPool,
    req: PostmanImportRequest,
    actor: &str,
) -> Result<DraftDto, ApiError> {
    let name = req
        .name
        .or_else(|| {
            req.collection
                .get("info")
                .and_then(|i| i.get("name"))
                .and_then(|v| v.as_str())
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "Postman Import".into());
    let draft_code = req
        .draft_code
        .unwrap_or_else(|| format!("pm-{}", &Uuid::new_v4().to_string()[..8]));

    let mut endpoints = Vec::new();
    let mut base_url = String::new();
    collect_postman_items(
        req.collection.get("item").unwrap_or(&serde_json::json!([])),
        &mut endpoints,
        &mut base_url,
        0,
    );

    create_draft(
        pool,
        CreateDraftRequest {
            draft_code,
            name,
            description: Some("Imported from Postman Collection".into()),
            upstream_type: "transparent".into(),
            upstream_url: if base_url.is_empty() {
                "https://api.example.com".into()
            } else {
                base_url
            },
            config: Some(serde_json::json!({"upstream_type":"transparent"})),
            endpoints: Some(serde_json::Value::Array(endpoints)),
            workbench_env: None,
            source: Some("postman".into()),
        },
        actor,
    )
    .await
}

pub(super) fn collect_postman_items(
    items: &serde_json::Value,
    out: &mut Vec<serde_json::Value>,
    base_url: &mut String,
    depth: usize,
) {
    if depth > 8 {
        return;
    }
    let Some(arr) = items.as_array() else {
        return;
    };
    for item in arr {
        if item.get("item").is_some() {
            collect_postman_items(&item["item"], out, base_url, depth + 1);
            continue;
        }
        let Some(req) = item.get("request") else {
            continue;
        };
        let method = req
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_string();
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("request")
            .to_string();
        let (url_base, path, query) = parse_postman_url(req.get("url"));
        if base_url.is_empty() && !url_base.is_empty() {
            *base_url = url_base;
        }
        let mut headers = serde_json::Map::new();
        if let Some(hs) = req.get("header").and_then(|v| v.as_array()) {
            for h in hs {
                if let (Some(k), Some(v)) = (
                    h.get("key").and_then(|x| x.as_str()),
                    h.get("value").and_then(|x| x.as_str()),
                ) {
                    headers.insert(k.to_string(), serde_json::Value::String(v.to_string()));
                }
            }
        }
        let body = req
            .get("body")
            .and_then(|b| b.get("raw"))
            .and_then(|v| v.as_str())
            .map(|s| {
                serde_json::from_str::<serde_json::Value>(s)
                    .unwrap_or(serde_json::Value::String(s.to_string()))
            });
        out.push(serde_json::json!({
            "id": format!("ep-{}", out.len() + 1),
            "name": name,
            "method": method,
            "path": path,
            "query": query,
            "headers": headers,
            "body": body,
            "assertions": [{"type":"status_range","min":200,"max":399}],
            "gate": true
        }));
    }
}

pub(super) fn parse_postman_url(
    url: Option<&serde_json::Value>,
) -> (String, String, serde_json::Map<String, serde_json::Value>) {
    let mut base = String::new();
    let mut path = "/".to_string();
    let mut query = serde_json::Map::new();
    let Some(url) = url else {
        return (base, path, query);
    };
    if let Some(s) = url.as_str() {
        if let Ok(u) = url::Url::parse(s) {
            base = format!(
                "{}://{}{}",
                u.scheme(),
                u.host_str().unwrap_or(""),
                u.port().map(|p| format!(":{p}")).unwrap_or_default()
            );
            path = if u.path().is_empty() {
                "/".into()
            } else {
                u.path().to_string()
            };
            for (k, v) in u.query_pairs() {
                query.insert(k.to_string(), serde_json::Value::String(v.to_string()));
            }
        }
        return (base, path, query);
    }
    if let Some(host) = url.get("host").and_then(|v| v.as_array()) {
        let host_s: String = host
            .iter()
            .filter_map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join(".");
        let protocol = url
            .get("protocol")
            .and_then(|v| v.as_str())
            .unwrap_or("https")
            .trim_end_matches(':');
        base = format!("{protocol}://{host_s}");
    }
    if let Some(parts) = url.get("path").and_then(|v| v.as_array()) {
        let p: String = parts
            .iter()
            .filter_map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join("/");
        path = format!("/{p}");
    } else if let Some(p) = url.get("path").and_then(|v| v.as_str()) {
        path = if p.starts_with('/') {
            p.to_string()
        } else {
            format!("/{p}")
        };
    }
    if let Some(qs) = url.get("query").and_then(|v| v.as_array()) {
        for q in qs {
            if let (Some(k), Some(v)) = (
                q.get("key").and_then(|x| x.as_str()),
                q.get("value").and_then(|x| x.as_str()),
            ) {
                query.insert(k.to_string(), serde_json::Value::String(v.to_string()));
            }
        }
    }
    (base, path, query)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn import_openapi(
    pool: &PgPool,
    req: OpenApiImportRequest,
    actor: &str,
) -> Result<DraftDto, ApiError> {
    let name = req
        .name
        .or_else(|| {
            req.document
                .get("info")
                .and_then(|i| i.get("title"))
                .and_then(|v| v.as_str())
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "OpenAPI Import".into());
    let draft_code = req
        .draft_code
        .unwrap_or_else(|| format!("oas-{}", &Uuid::new_v4().to_string()[..8]));

    let mut base_url = String::new();
    if let Some(servers) = req.document.get("servers").and_then(|v| v.as_array())
        && let Some(u) = servers
            .first()
            .and_then(|s| s.get("url"))
            .and_then(|v| v.as_str())
    {
        base_url = u.trim_end_matches('/').to_string();
    }

    let mut endpoints = Vec::new();
    if let Some(paths) = req.document.get("paths").and_then(|v| v.as_object()) {
        for (path, item) in paths {
            let Some(obj) = item.as_object() else {
                continue;
            };
            for method in ["get", "post", "put", "patch", "delete", "head", "options"] {
                if let Some(op) = obj.get(method) {
                    let op_name = op
                        .get("operationId")
                        .or_else(|| op.get("summary"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(method);
                    endpoints.push(serde_json::json!({
                        "id": format!("{}-{}", method, endpoints.len() + 1),
                        "name": op_name,
                        "method": method.to_uppercase(),
                        "path": path,
                        "assertions": [{"type":"status_range","min":200,"max":399}],
                        "gate": true
                    }));
                }
            }
        }
    }

    create_draft(
        pool,
        CreateDraftRequest {
            draft_code,
            name,
            description: Some("Imported from OpenAPI".into()),
            upstream_type: "transparent".into(),
            upstream_url: if base_url.is_empty() {
                "https://api.example.com".into()
            } else {
                base_url
            },
            config: Some(serde_json::json!({"upstream_type":"transparent"})),
            endpoints: Some(serde_json::Value::Array(endpoints)),
            workbench_env: None,
            source: Some("openapi".into()),
        },
        actor,
    )
    .await
}

/// 供反开工作台使用的渠道行快照。
#[derive(Debug, Clone)]
pub(crate) struct LiveChannelRow {
    pub channel_code: String,
    pub name: String,
    pub path_prefix: String,
    pub cost_price: Decimal,
    pub qps_limit: i32,
    pub config: serde_json::Value,
}

/// 格式化 `NUMERIC(18,6)` 成本单价字符串：保留有效小数位并去除冗余尾零，最少保留 2 位小数。
pub(crate) fn format_cost_price_decimal(value: Decimal) -> String {
    let raw = value.normalize().to_string();
    if let Some((int_part, frac_part)) = raw.split_once('.') {
        if frac_part.len() >= 2 {
            format!("{int_part}.{frac_part}")
        } else {
            format!("{int_part}.{frac_part:0<2}")
        }
    } else {
        format!("{raw}.00")
    }
}

/// 规范化供应商线上 `config` 以供工作台反开：
/// 原样保留加密密钥，且将“仅存有业务密钥仓、未声明任何 HTTP 换票/鉴权类型”的旧配置归一为 `enabled = false, profile = "none"`。
pub(crate) fn normalize_supplier_config_for_workbench(mut config: serde_json::Value) -> serde_json::Value {
    if let Some(auth_obj) = config
        .as_object_mut()
        .and_then(|o| o.get_mut("auth_flow"))
        .and_then(serde_json::Value::as_object_mut)
    {
        let profile_raw = auth_obj.get("profile").and_then(serde_json::Value::as_str);
        let token_url = auth_obj
            .get("token_url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let has_creds = auth_obj
            .get("credentials")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|m| !m.is_empty());
        if profile_is_undeclared(profile_raw) && token_url.is_empty() && has_creds {
            auth_obj.insert("enabled".to_string(), serde_json::Value::Bool(false));
            auth_obj.insert(
                "profile".to_string(),
                serde_json::Value::String("none".to_string()),
            );
        }
    }
    config
}

fn sorted_kv_entries_and_map(
    src: Option<&HashMap<String, String>>,
) -> (Vec<serde_json::Value>, serde_json::Map<String, serde_json::Value>) {
    let mut pairs: Vec<(&String, &String)> = src.map_or_else(Vec::new, |m| m.iter().collect());
    pairs.sort_by_key(|(a, _)| *a);
    let mut entries = Vec::with_capacity(pairs.len());
    let mut map = serde_json::Map::new();
    for (k, v) in pairs {
        entries.push(serde_json::json!({
            "key": k,
            "value": v,
            "enabled": true,
        }));
        map.insert(k.clone(), serde_json::Value::String(v.clone()));
    }
    (entries, map)
}

fn insert_optional_serialized<T: serde::Serialize>(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    val: Option<&T>,
) {
    if let Some(v) = val
        && let Ok(jv) = serde_json::to_value(v)
        && !jv.is_null()
    {
        obj.insert(key.to_string(), jv);
    }
}

/// 从单条线上 `channels` 记录（含 `config.egress`）完整还原工作台 `ConnectorEndpoint` JSON。
pub(crate) fn channel_row_to_workbench_endpoint(ch: &LiveChannelRow) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "id".into(),
        serde_json::Value::String(ch.channel_code.to_lowercase()),
    );
    obj.insert("name".into(), serde_json::Value::String(ch.name.clone()));
    obj.insert(
        "channel_code".into(),
        serde_json::Value::String(ch.channel_code.clone()),
    );
    obj.insert(
        "path_prefix".into(),
        serde_json::Value::String(ch.path_prefix.clone()),
    );
    obj.insert(
        "cost_price".into(),
        serde_json::Value::String(format_cost_price_decimal(ch.cost_price)),
    );
    obj.insert("qps_limit".into(), serde_json::json!(ch.qps_limit));

    if let Ok(spec) = parse_channel_config_egress(&ch.config) {
        apply_egress_spec_to_endpoint(&mut obj, &spec, &ch.path_prefix);
    } else {
        obj.insert("method".into(), serde_json::Value::String("POST".into()));
        obj.insert(
            "path".into(),
            serde_json::Value::String(if ch.path_prefix.trim().is_empty() {
                "/".into()
            } else {
                ch.path_prefix.clone()
            }),
        );
        obj.insert(
            "adapter".into(),
            serde_json::Value::String(ADAPTER_HTTP_JSON.into()),
        );
        obj.insert(
            "body_encoding".into(),
            serde_json::Value::String("json".into()),
        );
        obj.insert(
            "body_content_type".into(),
            serde_json::Value::String("json".into()),
        );
        obj.insert("body".into(), serde_json::json!({}));
    }

    obj.insert(
        "assertions".into(),
        serde_json::json!([{"type": "status_range", "min": 200, "max": 399}]),
    );
    obj.insert("gate".into(), serde_json::Value::Bool(true));
    serde_json::Value::Object(obj)
}

fn apply_egress_spec_to_endpoint(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    spec: &ChannelEgressSpec,
    fallback_path: &str,
) {
    let method = if spec.request.method.trim().is_empty() {
        "POST".to_string()
    } else {
        spec.request.method.trim().to_uppercase()
    };
    let path = if spec.request.path.trim().is_empty() {
        if fallback_path.trim().is_empty() {
            "/".to_string()
        } else {
            fallback_path.to_string()
        }
    } else {
        spec.request.path.clone()
    };
    let encoding_str = spec.request.body_encoding.as_str().to_string();
    obj.insert("method".into(), serde_json::Value::String(method));
    obj.insert("path".into(), serde_json::Value::String(path));
    obj.insert(
        "adapter".into(),
        serde_json::Value::String(if spec.adapter.trim().is_empty() {
            ADAPTER_HTTP_JSON.into()
        } else {
            spec.adapter.clone()
        }),
    );
    let l1_body_type = if spec.request.body_encoding == BodyEncoding::None {
        "none"
    } else {
        "json"
    };
    obj.insert(
        "body_encoding".into(),
        serde_json::Value::String(encoding_str),
    );
    obj.insert(
        "body_content_type".into(),
        serde_json::Value::String(l1_body_type.into()),
    );
    if let Some(ref ct) = spec.request.content_type
        && !ct.trim().is_empty()
    {
        obj.insert("content_type".into(), serde_json::Value::String(ct.clone()));
    }
    let (header_entries, headers_map) = sorted_kv_entries_and_map(spec.request.headers.as_ref());
    if !header_entries.is_empty() {
        obj.insert(
            "header_entries".into(),
            serde_json::Value::Array(header_entries),
        );
        obj.insert("headers".into(), serde_json::Value::Object(headers_map));
    }
    let (query_entries, query_map) = sorted_kv_entries_and_map(spec.request.query.as_ref());
    if !query_entries.is_empty() {
        obj.insert(
            "query_entries".into(),
            serde_json::Value::Array(query_entries),
        );
        obj.insert("query".into(), serde_json::Value::Object(query_map));
    }
    let sample_body = if spec.request.body_encoding == BodyEncoding::None {
        serde_json::Value::Null
    } else {
        serde_json::json!({})
    };
    obj.insert("body".into(), sample_body);

    if let Some(ref wrap) = spec.request.wrap
        && !wrap.trim().is_empty()
    {
        obj.insert("wrap".into(), serde_json::Value::String(wrap.clone()));
    }
    if let Some(unwrap) = spec.response.as_ref().and_then(|r| r.unwrap.as_deref())
        && !unwrap.trim().is_empty()
    {
        obj.insert(
            "unwrap".into(),
            serde_json::Value::String(unwrap.to_string()),
        );
    }
    if let Some(ref op) = spec.operation
        && !op.trim().is_empty()
    {
        obj.insert("operation".into(), serde_json::Value::String(op.clone()));
    }
    if let Some(ref aid) = spec.artifact_id
        && !aid.trim().is_empty()
    {
        obj.insert("artifact_id".into(), serde_json::Value::String(aid.clone()));
    }
    if let Some(ms) = spec.timeout_ms {
        obj.insert("timeout_ms".into(), serde_json::json!(ms));
    }
    insert_optional_serialized(obj, "auth", spec.auth.as_ref());
    insert_optional_serialized(obj, "crypto", spec.crypto.as_ref());
    insert_optional_serialized(obj, "graphql", spec.graphql.as_ref());
    insert_optional_serialized(obj, "job_poll", spec.job_poll.as_ref());
    insert_optional_serialized(obj, "sign_form", spec.sign_form.as_ref());
    insert_optional_serialized(obj, "soap", spec.soap.as_ref());
    insert_optional_serialized(obj, "ws", spec.ws.as_ref());
}

fn overlay_snapshot_metadata(
    endpoints: &mut [serde_json::Value],
    snapshot: Option<&serde_json::Value>,
) {
    let Some(snap_arr) = snapshot.and_then(serde_json::Value::as_array) else {
        return;
    };
    for ep in endpoints {
        let Some(ep_obj) = ep.as_object_mut() else {
            continue;
        };
        let ch_code = ep_obj
            .get("channel_code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let ep_id = ep_obj
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let matched = snap_arr.iter().find(|s| {
            let sc = s
                .get("channel_code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let sid = s.get("id").and_then(serde_json::Value::as_str).unwrap_or("");
            (!ch_code.is_empty() && sc.eq_ignore_ascii_case(ch_code))
                || (!ep_id.is_empty() && sid.eq_ignore_ascii_case(ep_id))
        });
        let Some(snap_ep) = matched else {
            continue;
        };
        if let Some(snap_id) = snap_ep.get("id").and_then(serde_json::Value::as_str)
            && !snap_id.trim().is_empty()
        {
            ep_obj.insert("id".into(), serde_json::Value::String(snap_id.to_string()));
        }
        if ep_obj.get("body").is_some_and(|b| b == &serde_json::json!({}))
            && let Some(snap_body) = snap_ep.get("body")
            && !snap_body.is_null()
        {
            ep_obj.insert("body".into(), snap_body.clone());
        }
        if let Some(assertions) = snap_ep.get("assertions")
            && assertions.is_array()
        {
            ep_obj.insert("assertions".into(), assertions.clone());
        }
        if let Some(gate) = snap_ep.get("gate").and_then(serde_json::Value::as_bool) {
            ep_obj.insert("gate".into(), serde_json::Value::Bool(gate));
        }
        if let Some(desc) = snap_ep.get("description").and_then(serde_json::Value::as_str)
            && !desc.trim().is_empty()
        {
            ep_obj.insert(
                "description".into(),
                serde_json::Value::String(desc.to_string()),
            );
        }
    }
}

async fn load_supplier_channels(
    pool: &PgPool,
    supplier_id: Uuid,
) -> Result<Vec<LiveChannelRow>, ApiError> {
    let rows = sqlx::query(
        r"
        SELECT channel_code, name, path_prefix, cost_price, qps_limit, config
        FROM channels
        WHERE supplier_id = $1
        ORDER BY channel_code
        ",
    )
    .bind(supplier_id)
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::internal(format!("list channels: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| LiveChannelRow {
            channel_code: r.get("channel_code"),
            name: r.get("name"),
            path_prefix: r.get("path_prefix"),
            cost_price: r.get("cost_price"),
            qps_limit: r.get("qps_limit"),
            config: r.get("config"),
        })
        .collect())
}

async fn build_supplier_endpoints_value(
    pool: &PgPool,
    supplier_id: Uuid,
) -> Result<serde_json::Value, ApiError> {
    let channels = load_supplier_channels(pool, supplier_id).await?;
    let latest_ver = repo::get_latest_version_for_supplier(pool, supplier_id)
        .await
        .map_err(|e| ApiError::internal(format!("latest version: {e}")))?;

    let mut endpoints: Vec<serde_json::Value> = channels
        .iter()
        .map(channel_row_to_workbench_endpoint)
        .collect();
    overlay_snapshot_metadata(
        &mut endpoints,
        latest_ver.as_ref().map(|v| &v.endpoints_snapshot),
    );
    if endpoints.is_empty() {
        if let Some(snap_arr) = latest_ver
            .as_ref()
            .and_then(|v| v.endpoints_snapshot.as_array())
            .filter(|a| !a.is_empty())
        {
            endpoints.clone_from(snap_arr);
        } else {
            endpoints.push(serde_json::json!({
                "id": "ep-1",
                "name": "默认接口",
                "method": "POST",
                "path": "/",
                "body_encoding": "json",
                "body_content_type": "json",
                "body": {},
                "assertions": [{"type": "status_range", "min": 200, "max": 399}],
                "gate": true
            }));
        }
    }
    Ok(serde_json::Value::Array(endpoints))
}

/// 只读反开供应商视图：若已有活跃草稿则返回该草稿，否则在内存构造脱敏只读快照（零落库写入，供观察者 `R_VIEWER` 使用）。
///
/// # Errors
///
/// 供应商不存在或底层数据库查询失败时返回错误。
pub async fn preview_draft_from_supplier(
    pool: &PgPool,
    supplier_id: Uuid,
) -> Result<DraftDto, ApiError> {
    let row = sqlx::query(
        r"
        SELECT supplier_code, name, upstream_url, config, created_at, updated_at
        FROM suppliers WHERE id = $1
        ",
    )
    .bind(supplier_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::internal(format!("get supplier: {e}")))?
    .ok_or_else(|| ApiError::not_found("supplier".into(), supplier_id.to_string()))?;

    if let Some(existing) = repo::find_active_draft_for_supplier(pool, supplier_id)
        .await
        .map_err(|e| ApiError::internal(format!("find active draft: {e}")))?
    {
        return with_gate(pool, existing).await;
    }

    let code: String = row.get("supplier_code");
    let name: String = row.get("name");
    let upstream_url: String = row.get("upstream_url");
    let raw_config: serde_json::Value = row.get("config");
    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
    let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");
    let normalized = normalize_supplier_config_for_workbench(raw_config);
    let upstream_type = normalized
        .get("upstream_type")
        .and_then(|v| v.as_str())
        .unwrap_or("transparent")
        .to_string();
    let config = mask_config_credentials(normalized);
    let endpoints = build_supplier_endpoints_value(pool, supplier_id).await?;

    Ok(DraftDto {
        id: Uuid::nil(),
        draft_code: format!("readonly-{code}"),
        name,
        description: Some(format!("只读查看供应商 {code}")),
        upstream_type,
        upstream_url,
        config,
        endpoints,
        workbench_env: serde_json::json!([]),
        status: "readonly".into(),
        source: "supplier".into(),
        linked_supplier_id: Some(supplier_id),
        created_by: None,
        created_at,
        updated_at,
        gate: Vec::new(),
    })
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn draft_from_supplier(
    pool: &PgPool,
    supplier_id: Uuid,
    force_resync: bool,
    actor: &str,
) -> Result<DraftDto, ApiError> {
    let row = sqlx::query(
        r"
        SELECT supplier_code, name, upstream_url, config, status
        FROM suppliers WHERE id = $1
        ",
    )
    .bind(supplier_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::internal(format!("get supplier: {e}")))?
    .ok_or_else(|| ApiError::not_found("supplier".into(), supplier_id.to_string()))?;

    let existing_draft = repo::find_active_draft_for_supplier(pool, supplier_id)
        .await
        .map_err(|e| ApiError::internal(format!("find active draft: {e}")))?;
    if !force_resync
        && let Some(existing) = existing_draft
    {
        return with_gate(pool, existing).await;
    }

    let code: String = row.get("supplier_code");
    let name: String = row.get("name");
    let upstream_url: String = row.get("upstream_url");
    let raw_config: serde_json::Value = row.get("config");
    let config = normalize_supplier_config_for_workbench(raw_config);
    let upstream_type = config
        .get("upstream_type")
        .and_then(|v| v.as_str())
        .unwrap_or("transparent")
        .to_string();
    let endpoints_value = build_supplier_endpoints_value(pool, supplier_id).await?;

    if let Some(existing) = existing_draft {
        let config_value = config_json_for_draft(&config, Some(&existing.config))?;
        let updated = repo::update_draft(
            pool,
            repo::DraftUpdate {
                id: existing.id,
                name: Some(&name),
                description: Some(&format!("反开自供应商 {code}")),
                upstream_type: Some(&upstream_type),
                upstream_url: Some(&upstream_url),
                config: Some(&config_value),
                endpoints: Some(&endpoints_value),
                workbench_env: None,
                status: Some("draft"),
            },
        )
        .await
        .map_err(|e| ApiError::internal(format!("resync draft: {e}")))?
        .ok_or_else(|| ApiError::not_found("draft".into(), existing.id.to_string()))?;
        return with_gate(pool, updated).await;
    }

    let draft_code = format!("from-{code}-{}", &Uuid::new_v4().to_string()[..6]);
    let mut dto = create_draft(
        pool,
        CreateDraftRequest {
            draft_code,
            name,
            description: Some(format!("反开自供应商 {code}")),
            upstream_type,
            upstream_url,
            config: Some(config),
            endpoints: Some(endpoints_value),
            workbench_env: None,
            source: Some("supplier".into()),
        },
        actor,
    )
    .await?;

    repo::set_draft_linked_supplier(pool, dto.id, supplier_id, "draft")
        .await
        .map_err(|e| ApiError::internal(format!("link supplier: {e}")))?;
    dto.linked_supplier_id = Some(supplier_id);
    Ok(dto)
}

fn extract_enabled_kv_items(
    ep: &serde_json::Value,
    entries_key: &str,
    map_key: &str,
) -> Vec<(String, String, Option<String>)> {
    if let Some(arr) = ep.get(entries_key).and_then(serde_json::Value::as_array) {
        let mut out = Vec::new();
        for row in arr {
            if row.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
                continue;
            }
            let Some(k) = row
                .get("key")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let v = row
                .get("value")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            let desc = row
                .get("desc")
                .or_else(|| row.get("description"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string);
            out.push((k.to_string(), v, desc));
        }
        return out;
    }
    if let Some(map) = ep.get(map_key).and_then(serde_json::Value::as_object) {
        let mut pairs: Vec<(&String, &serde_json::Value)> = map.iter().collect();
        pairs.sort_by_key(|(a, _)| *a);
        return pairs
            .into_iter()
            .filter(|(k, _)| !k.trim().is_empty())
            .map(|(k, v)| {
                let s = v
                    .as_str()
                    .map_or_else(|| v.to_string(), ToString::to_string);
                (k.clone(), s, None)
            })
            .collect();
    }
    Vec::new()
}

fn kv_items_to_postman(items: Vec<(String, String, Option<String>)>) -> Vec<serde_json::Value> {
    items
        .into_iter()
        .map(|(k, v, desc)| {
            let mut m = serde_json::Map::new();
            m.insert("key".into(), serde_json::Value::String(k));
            m.insert("value".into(), serde_json::Value::String(v));
            if let Some(d) = desc {
                m.insert("description".into(), serde_json::Value::String(d));
            }
            serde_json::Value::Object(m)
        })
        .collect()
}

fn build_postman_body(method: &str, ep: &serde_json::Value) -> Option<serde_json::Value> {
    let encoding = ep
        .get("body_encoding")
        .or_else(|| ep.get("body_content_type"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("json");
    if encoding == "none" || matches!(method, "GET" | "HEAD") {
        return None;
    }
    if encoding == "form" {
        let mut form_items = Vec::new();
        if let Some(body_obj) = ep.get("body").and_then(serde_json::Value::as_object) {
            for (k, v) in body_obj {
                let s = v
                    .as_str()
                    .map_or_else(|| v.to_string(), ToString::to_string);
                form_items.push(serde_json::json!({
                    "key": k,
                    "value": s,
                    "type": "text"
                }));
            }
        }
        return Some(serde_json::json!({
            "mode": "urlencoded",
            "urlencoded": form_items
        }));
    }
    let body_val = ep.get("body").filter(|v| !v.is_null())?;
    let raw_body = match body_val {
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    };
    Some(serde_json::json!({
        "mode": "raw",
        "raw": raw_body,
        "options": { "raw": { "language": "json" } }
    }))
}

pub(crate) fn export_postman_item(upstream_url: &str, ep: &serde_json::Value) -> serde_json::Value {
    let method = ep
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("GET")
        .to_uppercase();
    let path = ep
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("/");
    let name = ep
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(path);
    let headers = kv_items_to_postman(extract_enabled_kv_items(ep, "header_entries", "headers"));

    let query_items = extract_enabled_kv_items(ep, "query_entries", "query");
    let base_url = format!("{}{}", upstream_url.trim_end_matches('/'), path);
    let raw_url = if query_items.is_empty() {
        base_url
    } else {
        let qs: String = query_items
            .iter()
            .map(|(k, v, _)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        format!("{base_url}?{qs}")
    };
    let pm_query = kv_items_to_postman(query_items);

    let mut url_obj = serde_json::Map::new();
    url_obj.insert("raw".into(), serde_json::Value::String(raw_url));
    if !pm_query.is_empty() {
        url_obj.insert("query".into(), serde_json::Value::Array(pm_query));
    }

    let mut request = serde_json::Map::new();
    request.insert("method".into(), serde_json::Value::String(method.clone()));
    request.insert("header".into(), serde_json::Value::Array(headers));
    request.insert("url".into(), serde_json::Value::Object(url_obj));
    if let Some(desc) = ep
        .get("description")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        request.insert(
            "description".into(),
            serde_json::Value::String(desc.to_string()),
        );
    }
    if let Some(body) = build_postman_body(&method, ep) {
        request.insert("body".into(), body);
    }

    serde_json::json!({
        "name": name,
        "request": serde_json::Value::Object(request),
    })
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn export_postman(pool: &PgPool, id: Uuid) -> Result<serde_json::Value, ApiError> {
    let d = get_draft(pool, id).await?;
    let mut items = Vec::new();
    if let Some(arr) = d.endpoints.as_array() {
        for ep in arr {
            items.push(export_postman_item(&d.upstream_url, ep));
        }
    }
    Ok(serde_json::json!({
        "info": {
            "name": d.name,
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        },
        "item": items
    }))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn transform_preview(
    pool: &PgPool,
    engine: &RuntimeEngine,
    id: Uuid,
    req: RunRequest,
) -> Result<serde_json::Value, ApiError> {
    let row = repo::get_draft(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("get draft: {e}")))?
        .ok_or_else(|| ApiError::not_found("draft".into(), id.to_string()))?;

    let mut cfg = SupplierConfig::parse_and_validate(&row.config)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    cfg = cfg
        .decrypt_credentials_for_runtime()
        .map_err(|e| ApiError::internal(format!("decrypt: {e}")))?;

    let env = parse_workbench_env(&row.workbench_env);
    let ep = endpoint_by_id(&row.endpoints, req.endpoint_id.as_deref());
    let empty_ep = serde_json::json!({});
    let built = build_workbench_spec(ep.unwrap_or(&empty_ep), &env, Some(&cfg), &row.upstream_url)?;
    let payload = built.payload.clone();
    let endpoint_key = ep.and_then(|e| workbench_endpoint_key(e, req.endpoint_id.as_deref()));
    let spec = built.spec.clone();

    let mut credentials = HashMap::new();
    if let Some(ref auth) = cfg.auth_flow {
        credentials.clone_from(&auth.credentials);
    }

    let input = InvokeInput {
        supplier_id: row
            .linked_supplier_id
            .map(|u| u.to_string())
            .unwrap_or_default(),
        supplier_code: format!("preview:{}", row.draft_code),
        channel_id: endpoint_key.unwrap_or_else(|| row.id.to_string()),
        request_id: Uuid::new_v4().to_string(),
        payload,
        spec,
        config: Some(Arc::new(cfg)),
        timeout: Duration::from_secs(5),
        mode: ExecuteMode::TransformPreview,
        options: ExecuteOptions {
            follow_redirects: false,
            max_body_capture_bytes: 64 * 1024,
            mask_secrets: true,
            collect_trace: true,
        },
        pipeline_keys: Some(mazbri_upstream_runtime::pipeline_keys_from_credentials(&credentials)),
        credentials,
        artifact_bytes: None,
        cache_key: None,
    };
    let result = engine.invoke(input).await;
    Ok(serde_json::json!({
        "ok": result.ok,
        "stages": result.stages,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn sample_sm4_live_channel() -> LiveChannelRow {
        LiveChannelRow {
            channel_code: "UPUPT-ENV-SC-SM4-DEFAULT".to_string(),
            name: "国密SM4信封通道".to_string(),
            path_prefix: "/".to_string(),
            cost_price: Decimal::from_str("0.002500").unwrap(),
            qps_limit: 50,
            config: serde_json::json!({
                "egress": {
                    "adapter": "http_envelope",
                    "operation": "queryOrder",
                    "base_url": "https://partner.example.com",
                    "request": {
                        "method": "POST",
                        "path": "/sm4",
                        "body_encoding": "json",
                        "headers": { "X-Partner": "mazbri" },
                        "query": { "ver": "2" },
                        "wrap": "method_biz_content"
                    },
                    "response": { "unwrap": "$.data" },
                    "crypto": {
                        "version": 1,
                        "order": "encrypt_then_sign",
                        "canon": "sorted_kv",
                        "encodings": {
                            "payload": { "hex": { "case": "lower" } },
                            "signature": "base64",
                            "iv": "base64"
                        },
                        "slots": [
                            { "role": "sign", "location": "json", "name": "sign" },
                            { "role": "cipher", "location": "json", "name": "biz_content" }
                        ],
                        "iv": "random",
                        "scope": "whole_body",
                        "allow_insecure": false,
                        "crypt": { "alg": "sm4-cbc-pkcs7" },
                        "sign": { "alg": "hmac-sm3" }
                    }
                }
            }),
        }
    }

    #[test]
    fn reverse_import_restores_sm4_envelope_and_roundtrips_spec() {
        let live_channel = sample_sm4_live_channel();

        let mut endpoints = vec![channel_row_to_workbench_endpoint(&live_channel)];
        let snapshot = serde_json::json!([
            {
                "channel_code": "UPUPT-ENV-SC-SM4-DEFAULT",
                "body": { "orderNo": "ORD-001" }
            }
        ]);
        overlay_snapshot_metadata(&mut endpoints, Some(&snapshot));
        let ep = &endpoints[0];
        assert_eq!(ep["method"], "POST");
        assert_eq!(ep["path"], "/sm4");
        assert_eq!(ep["path_prefix"], "/");
        assert_eq!(ep["cost_price"], "0.0025");
        assert_eq!(ep["qps_limit"], 50);
        assert_eq!(ep["adapter"], "http_envelope");
        assert_eq!(ep["operation"], "queryOrder");
        assert_eq!(ep["wrap"], "method_biz_content");
        assert_eq!(ep["unwrap"], "$.data");
        assert_eq!(ep["crypto"]["crypt"]["alg"], "sm4-cbc-pkcs7");
        assert_eq!(ep["crypto"]["sign"]["alg"], "hmac-sm3");
        assert_eq!(ep["crypto"]["slots"][1]["name"], "biz_content");
        assert_eq!(ep["body"]["orderNo"], "ORD-001");

        let raw_supplier_cfg = serde_json::json!({
            "upstream_type": "transparent",
            "auth_flow": {
                "enabled": true,
                "credentials": {
                    "crypto_data_key": "enc:v2:aes-256-gcm:demo1",
                    "crypto_mac_key": "enc:v2:aes-256-gcm:demo2"
                }
            }
        });
        let normalized_cfg = normalize_supplier_config_for_workbench(raw_supplier_cfg);
        assert_eq!(normalized_cfg["auth_flow"]["enabled"], false);
        assert_eq!(normalized_cfg["auth_flow"]["profile"], "none");
        assert_eq!(
            normalized_cfg["auth_flow"]["credentials"]["crypto_data_key"],
            "enc:v2:aes-256-gcm:demo1"
        );

        let parsed_cfg = SupplierConfig::parse_and_validate(&normalized_cfg).expect("valid config");
        let rebuilt = build_workbench_spec(
            ep,
            &HashMap::new(),
            Some(&parsed_cfg),
            "https://partner.example.com",
        )
        .expect("rebuilt spec must succeed without auth deadlock");
        assert_eq!(rebuilt.spec.request.method, "POST");
        assert_eq!(rebuilt.spec.request.path, "/sm4");
        assert_eq!(rebuilt.spec.adapter, "http_envelope");
        assert!(rebuilt.spec.crypto.is_some());
        assert!(rebuilt.spec.auth.is_none());
    }

    #[test]
    fn export_postman_item_serializes_headers_query_and_body() {
        let ep = serde_json::json!({
            "name": "查询订单",
            "description": "SM4 查询接口",
            "method": "POST",
            "path": "/sm4",
            "body_encoding": "json",
            "header_entries": [
                { "key": "X-App", "value": "demo", "description": "应用标识", "enabled": true },
                { "key": "X-Disabled", "value": "no", "enabled": false }
            ],
            "query_entries": [
                { "key": "v", "value": "1", "enabled": true }
            ],
            "body": { "orderId": "1001" }
        });
        let item = export_postman_item("https://partner.example.com/", &ep);
        assert_eq!(item["name"], "查询订单");
        assert_eq!(item["request"]["method"], "POST");
        assert_eq!(item["request"]["description"], "SM4 查询接口");
        assert_eq!(
            item["request"]["url"]["raw"],
            "https://partner.example.com/sm4?v=1"
        );
        assert_eq!(item["request"]["header"].as_array().unwrap().len(), 1);
        assert_eq!(item["request"]["header"][0]["key"], "X-App");
        assert_eq!(item["request"]["body"]["mode"], "raw");
    }
}
