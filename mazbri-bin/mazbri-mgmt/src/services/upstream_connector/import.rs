use super::{
    ApiError, Arc, CreateDraftRequest, CurlImportRequest, DraftDto, Duration, ExecuteMode,
    ExecuteOptions, HashMap, InvokeInput, OpenApiImportRequest, PgPool, PostmanImportRequest, Row,
    RunRequest, SupplierConfig, Uuid, build_workbench_spec, create_draft, endpoint_by_id,
    get_draft, parse_workbench_env, repo, runtime_engine, workbench_endpoint_key,
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
        if t == "-X" || t == "--request" {
            if let Some(m) = tokens.get(i + 1) {
                method = m.to_uppercase();
                i += 2;
                continue;
            }
        }
        if t == "-H" || t == "--header" {
            if let Some(h) = tokens.get(i + 1) {
                if let Some((k, v)) = h.split_once(':') {
                    headers.insert(k.trim().to_string(), v.trim().to_string());
                }
                i += 2;
                continue;
            }
        }
        if t == "-d" || t == "--data" || t == "--data-raw" || t == "--data-binary" {
            if let Some(d) = tokens.get(i + 1) {
                body = Some(d.clone());
                if method == "GET" {
                    method = "POST".into();
                }
                i += 2;
                continue;
            }
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
    if let Some(servers) = req.document.get("servers").and_then(|v| v.as_array()) {
        if let Some(u) = servers
            .first()
            .and_then(|s| s.get("url"))
            .and_then(|v| v.as_str())
        {
            base_url = u.trim_end_matches('/').to_string();
        }
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

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn draft_from_supplier(
    pool: &PgPool,
    supplier_id: Uuid,
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

    let code: String = row.get("supplier_code");
    let name: String = row.get("name");
    let upstream_url: String = row.get("upstream_url");
    let config: serde_json::Value = row.get("config");
    let upstream_type = config
        .get("upstream_type")
        .and_then(|v| v.as_str())
        .unwrap_or("transparent")
        .to_string();

    let channels: Vec<(String, String, String)> = sqlx::query_as::<_, (String, String, String)>(
        "SELECT channel_code, name, path_prefix FROM channels WHERE supplier_id = $1 ORDER BY channel_code",
    )
    .bind(supplier_id)
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::internal(format!("list channels: {e}")))?;

    let endpoints: Vec<serde_json::Value> = channels
        .into_iter()
        .map(|(cc, cn, pp)| {
            serde_json::json!({
                "id": cc.to_lowercase(),
                "name": cn,
                "method": "GET",
                "path": pp,
                "channel_code": cc,
                "path_prefix": pp,
                "assertions": [{"type":"status_range","min":200,"max":399}],
                "gate": true
            })
        })
        .collect();

    let draft_code = format!("from-{code}-{}", &Uuid::new_v4().to_string()[..6]);
    let mut dto = create_draft(
        pool,
        CreateDraftRequest {
            draft_code,
            name: format!("{name} (from live)"),
            description: Some(format!("反开自供应商 {code}")),
            upstream_type,
            upstream_url,
            config: Some(config),
            endpoints: Some(serde_json::Value::Array(endpoints)),
            workbench_env: None,
            source: Some("supplier".into()),
        },
        actor,
    )
    .await?;

    // link
    repo::set_draft_linked_supplier(pool, dto.id, supplier_id, "draft")
        .await
        .map_err(|e| ApiError::internal(format!("link supplier: {e}")))?;
    dto.linked_supplier_id = Some(supplier_id);
    Ok(dto)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn export_postman(pool: &PgPool, id: Uuid) -> Result<serde_json::Value, ApiError> {
    let d = get_draft(pool, id).await?;
    let mut items = Vec::new();
    if let Some(arr) = d.endpoints.as_array() {
        for ep in arr {
            let method = ep.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
            let path = ep.get("path").and_then(|v| v.as_str()).unwrap_or("/");
            let name = ep.get("name").and_then(|v| v.as_str()).unwrap_or(path);
            items.push(serde_json::json!({
                "name": name,
                "request": {
                    "method": method,
                    "header": [],
                    "url": format!("{}{}", d.upstream_url.trim_end_matches('/'), path)
                }
            }));
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
        credentials,
        artifact_bytes: None,
        cache_key: None,
    };
    let result = runtime_engine().invoke(input).await;
    Ok(serde_json::json!({
        "ok": result.ok,
        "stages": result.stages,
    }))
}
