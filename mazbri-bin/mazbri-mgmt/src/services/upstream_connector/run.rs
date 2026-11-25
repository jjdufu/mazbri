use super::{
    ApiError, Arc, Assertion, AssertionResult, AuditAction, AuditLogRequest, AuditResourceType,
    AuditService, DEFAULT_RUN_LIMIT, Duration, ExecuteMode, ExecuteOptions, HashMap, InvokeInput,
    OutboundResponse, PgPool, RecentGateRun, RunDto, RunRequest, RuntimeEngine, SsrfChecker,
    SupplierConfig, UPSTREAM_TYPE_TRANSPARENT, Uuid, build_workbench_spec_with_captures,
    connector_metrics, endpoint_by_id, evaluate_assertions, evaluate_gate_items_for_row,
    load_artifact_bytes, mask_config_credentials, materialize_snapshot_row,
    parse_workbench_env_with_id, repo, ssrf, workbench_endpoint_key,
};
use crate::services::bypass_probe::{
    self, BypassProbeClient, PROBE_PATH_BYPASS_MGMT_PROBE, ProbeInvokeParams, ProbeOutcome,
};
use mazbri_core::hash_egress_spec_json;
use mazbri_core::json_path::extract_by_path;
use mazbri_core::num::SaturatingFrom;
use mazbri_upstream_runtime::mask_headers;
use std::collections::BTreeMap;

// ---------- Validate / Run ----------

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn validate_draft(
    pool: &PgPool,
    checker: &SsrfChecker,
    id: Uuid,
) -> Result<serde_json::Value, ApiError> {
    let row = repo::get_draft(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("get draft: {e}")))?
        .ok_or_else(|| ApiError::not_found("draft".into(), id.to_string()))?;

    let mut warnings: Vec<String> = Vec::new();
    let cfg = SupplierConfig::parse_and_validate(&row.config)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    cfg.validate_upstream_url_for_type(&row.upstream_url)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;

    if cfg.is_transparent() {
        ssrf::validate_public_http_url(checker, &row.upstream_url)
            .await
            .map_err(|e| ApiError::validation(format!("SSRF_BLOCKED: {e}")))?;
        if let Some(ref auth) = cfg.auth_flow
            && auth.enabled
            && !auth.token_url.trim().is_empty()
        {
            ssrf::validate_public_http_url(checker, &auth.token_url)
                .await
                .map_err(|e| ApiError::validation(format!("SSRF_BLOCKED token_url: {e}")))?;
        }
    } else {
        warnings.push("microservice: 跳过 HTTP SSRF 校验".into());
    }

    let endpoints = row.endpoints.as_array().map_or(0, std::vec::Vec::len);
    Ok(serde_json::json!({
        "valid": true,
        "upstream_type": row.upstream_type,
        "endpoints_count": endpoints,
        "warnings": warnings,
        "config": mask_config_credentials(row.config),
    }))
}

/// `run_draft` 的入参。
pub struct RunDraftParams<'a> {
    pub audit: &'a AuditService,
    /// 仅 `auth_only` / `transform_preview`（及 SSRF 预检）使用；**不得**再作 business/live 默认 Send。
    pub engine: &'a RuntimeEngine,
    /// business/live 经 bypass 隔离管理探针 HTTP（不经 gateway 计费）。
    pub bypass: &'a BypassProbeClient,
    pub id: Option<Uuid>,
    pub req: RunRequest,
    pub actor: &'a str,
    pub user_id: Uuid,
    pub is_super_admin: bool,
    pub client_ip: Option<String>,
    pub default_timeout_ms: u64,
    pub max_body_bytes: usize,
}

pub(super) async fn resolve_run_row(
    pool: &PgPool,
    id: Option<Uuid>,
    req: &RunRequest,
    is_super_admin: bool,
) -> Result<repo::DraftRow, ApiError> {
    if let Some(did) = id {
        let existing = repo::get_draft(pool, did)
            .await
            .map_err(|e| ApiError::internal(format!("get draft: {e}")))?
            .ok_or_else(|| ApiError::not_found("draft".into(), did.to_string()))?;
        if let Some(ref snap) = req.draft_snapshot {
            return materialize_snapshot_row(snap, Some(&existing), is_super_admin);
        }
        return Ok(existing);
    }
    let snap = req
        .draft_snapshot
        .as_ref()
        .ok_or_else(|| ApiError::validation("未保存草稿试调必须提供 draft_snapshot"))?;
    materialize_snapshot_row(snap, None, is_super_admin)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn run_draft(pool: &PgPool, params: RunDraftParams<'_>) -> Result<RunDto, ApiError> {
    let RunDraftParams {
        audit,
        engine,
        bypass,
        id,
        req,
        actor,
        user_id,
        is_super_admin,
        client_ip,
        default_timeout_ms,
        max_body_bytes,
    } = params;
    let row = resolve_run_row(pool, id, &req, is_super_admin).await?;

    // Microservice: connectivity / contract probe only (no HTTP auth/business pipeline).
    if row.upstream_type != UPSTREAM_TYPE_TRANSPARENT {
        if matches!(req.mode.as_str(), "business" | "auth_only" | "live") {
            return Err(ApiError::validation(
                "microservice 模式请使用 mode=probe_connectivity 或 probe_contract，不支持 HTTP business/auth 试跑",
            ));
        }
        return probe_microservice_draft(
            pool,
            ProbeMicroserviceParams {
                audit,
                ssrf: engine.ssrf(),
                draft_id: id,
                row: &row,
                req: &req,
                actor,
                user_id,
                client_ip,
                default_timeout_ms,
            },
        )
        .await;
    }

    match req.mode.as_str() {
        "auth_only" | "transform_preview" => {
            run_draft_local_non_gate(
                pool,
                LocalNonGateParams {
                    audit,
                    engine,
                    id,
                    row: &row,
                    req: &req,
                    actor,
                    user_id,
                    client_ip,
                    default_timeout_ms,
                    max_body_bytes,
                },
            )
            .await
        }
        "business" | "live" => {
            run_draft_via_bypass(
                pool,
                BypassRunParams {
                    audit,
                    bypass,
                    ssrf: engine.ssrf(),
                    id,
                    row: &row,
                    req: &req,
                    actor,
                    user_id,
                    client_ip,
                    default_timeout_ms,
                    max_body_bytes,
                },
            )
            .await
        }
        other => Err(ApiError::validation(format!(
            "unsupported run mode: {other}（business/live 经 bypass；auth_only/transform_preview 本地；无 engine 兜底）"
        ))),
    }
}

/// `probe_microservice_draft` 的入参。
pub(super) struct ProbeMicroserviceParams<'a> {
    pub(super) audit: &'a AuditService,
    pub(super) ssrf: &'a SsrfChecker,
    pub(super) draft_id: Option<Uuid>,
    pub(super) row: &'a repo::DraftRow,
    pub(super) req: &'a RunRequest,
    pub(super) actor: &'a str,
    pub(super) user_id: Uuid,
    pub(super) client_ip: Option<String>,
    pub(super) default_timeout_ms: u64,
}

/// Microservice draft probe: TCP reachability + optional `contract_id` presence check.
pub(super) async fn probe_microservice_draft(
    pool: &PgPool,
    params: ProbeMicroserviceParams<'_>,
) -> Result<RunDto, ApiError> {
    let ProbeMicroserviceParams {
        audit,
        ssrf,
        draft_id,
        row,
        req,
        actor,
        user_id,
        client_ip,
        default_timeout_ms,
    } = params;
    let start = std::time::Instant::now();

    let cfg = SupplierConfig::parse_and_validate(&row.config)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    let (contract_ok, validate_stage) = probe_contract_stage(&cfg);
    let (connect_ok, connect_stage) =
        probe_connectivity_stage(ssrf, &row.upstream_url, default_timeout_ms).await;
    let stages = vec![validate_stage, connect_stage];

    let overall_ok = contract_ok && connect_ok;
    let latency_ms = i32::saturating_from(start.elapsed().as_millis());
    let mode = if req.mode.is_empty() {
        "probe_connectivity"
    } else {
        req.mode.as_str()
    };
    let stages_json = serde_json::Value::Array(stages);
    let error_code = if overall_ok {
        None
    } else if !contract_ok {
        Some("CONFIG_INVALID")
    } else {
        Some("PROBE_CONNECT_FAILED")
    };

    connector_metrics::record_run(mode, overall_ok, error_code);

    let run_row = repo::insert_run(
        pool,
        repo::NewRun {
            draft_id,
            supplier_id: None,
            mode,
            ok: overall_ok,
            latency_ms: Some(latency_ms),
            stages: &stages_json,
            assertions: &serde_json::json!([]),
            error_code,
            error_message: if overall_ok {
                None
            } else {
                Some("microservice probe failed")
            },
            endpoint_key: None,
            testcase_id: None,
            actor: Some(actor),
            spec_fingerprint: None,
            probe_path: None,
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("insert run: {e}")))?;

    audit.log(AuditLogRequest {
        user_id: Some(user_id),
        action: AuditAction::ConnectorRun,
        resource_type: AuditResourceType::Supplier,
        resource_id: draft_id.map(|u| u.to_string()),
        details: Some(serde_json::json!({
            "run_id": run_row.id,
            "mode": mode,
            "ok": overall_ok,
            "upstream_type": "microservice",
        })),
        ip_address: client_ip,
        ..Default::default()
    });

    Ok(RunDto {
        id: run_row.id,
        draft_id,
        supplier_id: None,
        mode: mode.into(),
        ok: overall_ok,
        latency_ms: Some(latency_ms),
        stages: stages_json,
        assertions: serde_json::json!([]),
        error_code: error_code.map(std::string::ToString::to_string),
        error_message: if overall_ok {
            None
        } else {
            Some("microservice probe failed".into())
        },
        endpoint_key: None,
        actor: Some(actor.to_string()),
        created_at: run_row.created_at,
        response_preview: None,
        spec_fingerprint: None,
        probe_path: None,
        gate: Vec::new(),
    })
}

fn invoke_assert_response(
    result: &mazbri_upstream_runtime::InvokeResult,
) -> Option<OutboundResponse> {
    if let Some(ref p) = result.payload {
        let mut r = result.response.clone().unwrap_or(OutboundResponse {
            status: 200,
            headers: HashMap::default(),
            body: Vec::new(),
            final_url: String::new(),
            method: "POST".into(),
            path: "/".into(),
            protocol: String::new(),
            wire_duration: Duration::ZERO,
        });
        r.body = serde_json::to_vec(p).unwrap_or_default();
        if result.biz_error.is_some() {
            r.status = 400;
        }
        return Some(r);
    }
    result.response.clone()
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_runs(pool: &PgPool, draft_id: Uuid) -> Result<Vec<RunDto>, ApiError> {
    let rows = repo::list_runs_for_draft(pool, draft_id, DEFAULT_RUN_LIMIT)
        .await
        .map_err(|e| ApiError::internal(format!("list runs: {e}")))?;
    Ok(rows
        .into_iter()
        .map(|r| RunDto {
            id: r.id,
            draft_id: r.draft_id,
            supplier_id: r.supplier_id,
            mode: r.mode,
            ok: r.ok,
            latency_ms: r.latency_ms,
            stages: r.stages,
            assertions: r.assertions,
            error_code: r.error_code,
            error_message: r.error_message,
            endpoint_key: r.endpoint_key,
            actor: r.actor,
            created_at: r.created_at,
            response_preview: None,
            spec_fingerprint: r.spec_fingerprint,
            probe_path: r.probe_path,
            gate: Vec::new(),
        })
        .collect())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_run(pool: &PgPool, run_id: Uuid) -> Result<RunDto, ApiError> {
    let r = repo::get_run(pool, run_id)
        .await
        .map_err(|e| ApiError::internal(format!("get run: {e}")))?
        .ok_or_else(|| ApiError::not_found("run".into(), run_id.to_string()))?;
    Ok(RunDto {
        id: r.id,
        draft_id: r.draft_id,
        supplier_id: r.supplier_id,
        mode: r.mode,
        ok: r.ok,
        latency_ms: r.latency_ms,
        stages: r.stages,
        assertions: r.assertions,
        error_code: r.error_code,
        error_message: r.error_message,
        endpoint_key: r.endpoint_key,
        actor: r.actor,
        created_at: r.created_at,
        response_preview: None,
        spec_fingerprint: r.spec_fingerprint,
        probe_path: r.probe_path,
        gate: Vec::new(),
    })
}

/// 按请求或端点上配置的断言评估结果；无 HTTP 响应的失败不评估。
fn run_draft_assertions(
    req: &RunRequest,
    endpoints: &serde_json::Value,
    result: &mazbri_upstream_runtime::InvokeResult,
) -> Vec<AssertionResult> {
    let has_http_status = result
        .upstream_meta
        .as_ref()
        .map(|m| m.status)
        .or_else(|| result.response.as_ref().map(|r| r.status))
        .is_some();
    if !result.ok && !has_http_status {
        return Vec::new();
    }
    let raw: Vec<serde_json::Value> = if let Some(ref raw_assertions) = req.assertions {
        raw_assertions.clone()
    } else {
        let Some(ep_id) = req.endpoint_id.as_deref() else {
            return Vec::new();
        };
        let Some(arr) = endpoints
            .as_array()
            .and_then(|eps| {
                eps.iter().find(|e| {
                    e.get("id").and_then(|v| v.as_str()) == Some(ep_id)
                        || e.get("name").and_then(|v| v.as_str()) == Some(ep_id)
                })
            })
            .and_then(|ep| ep.get("assertions"))
            .and_then(|v| v.as_array())
        else {
            return Vec::new();
        };
        arr.clone()
    };
    let parsed: Vec<Assertion> = raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    let assert_response = invoke_assert_response(result);
    evaluate_assertions(&parsed, assert_response.as_ref(), result.latency_ms)
}

fn run_error_code(
    result: &mazbri_upstream_runtime::InvokeResult,
    overall_ok: bool,
    assertions_ok: bool,
) -> Option<&str> {
    if overall_ok {
        None
    } else if !result.ok {
        result.error_code.as_deref()
    } else if !assertions_ok {
        Some("ASSERTION_FAILED")
    } else {
        None
    }
}

fn extract_endpoint_captures(
    ep: Option<&serde_json::Value>,
    payload: Option<&serde_json::Value>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let (Some(ep), Some(body)) = (ep, payload) else {
        return out;
    };
    let Some(items) = ep.get("captures").and_then(serde_json::Value::as_array) else {
        return out;
    };
    for row in items {
        if row.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
            continue;
        }
        let name = row
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let raw_path = row
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        if name.is_empty() || raw_path.is_empty() {
            continue;
        }
        let norm_path = if raw_path == "$" || raw_path.starts_with("$.") {
            raw_path.to_string()
        } else {
            format!("$.{raw_path}")
        };
        if let Some(val) = extract_by_path(body, &norm_path)
            && !val.is_null()
        {
            let text = match val {
                serde_json::Value::String(s) => s,
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                other => other.to_string(),
            };
            out.insert(name.to_string(), text);
        }
    }
    out
}

fn run_response_preview(
    result: &mazbri_upstream_runtime::InvokeResult,
    request_id: &str,
    captured: &BTreeMap<String, String>,
) -> Option<serde_json::Value> {
    let resp_headers = result
        .response
        .as_ref()
        .map_or_default(|r| mask_headers(&r.headers, true));
    let upstream_req_id = result
        .upstream_meta
        .as_ref()
        .map(|m| m.request_id.trim().to_string())
        .filter(|s| !s.is_empty());
    result
        .payload
        .as_ref()
        .map(|p| {
            let status = result
                .upstream_meta
                .as_ref()
                .map_or(if result.ok { 200 } else { 400 }, |m| m.status);
            serde_json::json!({
                "status": status,
                "headers": resp_headers,
                "body_preview": p.to_string().chars().take(4096).collect::<String>(),
                "url": result.upstream_meta.as_ref().map_or_default(|m| m.url.clone()),
                "adapter": result.upstream_meta.as_ref().map(|m| m.adapter.clone()),
                "biz_error": result.biz_error,
                "request_id": request_id,
                "upstream_request_id": upstream_req_id,
                "captured": captured,
            })
        })
        .or_else(|| {
            result.response.as_ref().map(|r| {
                serde_json::json!({
                    "status": r.status,
                    "headers": mask_headers(&r.headers, true),
                    "body_preview": String::from_utf8_lossy(&r.body).chars().take(4096).collect::<String>(),
                    "url": r.final_url,
                    "request_id": request_id,
                    "upstream_request_id": upstream_req_id,
                    "captured": captured,
                })
            })
        })
}


/// `auth_only` / `transform_preview`：mgmt 本地 `RuntimeEngine`；`probe_path` 为 `NULL`，永不进 gate。
struct LocalNonGateParams<'a> {
    audit: &'a AuditService,
    engine: &'a RuntimeEngine,
    id: Option<Uuid>,
    row: &'a repo::DraftRow,
    req: &'a RunRequest,
    actor: &'a str,
    user_id: Uuid,
    client_ip: Option<String>,
    default_timeout_ms: u64,
    max_body_bytes: usize,
}

async fn validate_auth_only_ssrf(
    ssrf: &SsrfChecker,
    fallback_upstream_url: &str,
    input: &InvokeInput,
) -> Result<(), ApiError> {
    let target_base = if input.spec.base_url.trim().is_empty() {
        fallback_upstream_url
    } else {
        input.spec.base_url.as_str()
    };
    if !target_base.trim().is_empty() {
        ssrf::validate_public_http_url(ssrf, target_base)
            .await
            .map_err(|e| ApiError::validation(format!("SSRF_BLOCKED: {e}")))?;
    }
    if let Some(token_url) = input
        .spec
        .auth
        .as_ref()
        .and_then(|a| a.token_url.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        ssrf::validate_public_http_url(ssrf, token_url)
            .await
            .map_err(|e| ApiError::validation(format!("SSRF_BLOCKED token_url: {e}")))?;
    }
    if let Some(token_url) = input
        .config
        .as_ref()
        .and_then(|c| c.auth_flow.as_ref())
        .filter(|a| a.enabled)
        .map(|a| a.token_url.trim())
        .filter(|s| !s.is_empty())
    {
        ssrf::validate_public_http_url(ssrf, token_url)
            .await
            .map_err(|e| ApiError::validation(format!("SSRF_BLOCKED token_url: {e}")))?;
    }
    Ok(())
}

async fn run_draft_local_non_gate(
    pool: &PgPool,
    params: LocalNonGateParams<'_>,
) -> Result<RunDto, ApiError> {
    let LocalNonGateParams {
        audit,
        engine,
        id,
        row,
        req,
        actor,
        user_id,
        client_ip,
        default_timeout_ms,
        max_body_bytes,
    } = params;
    let PreparedRun {
        input,
        endpoint_key,
        request_id,
    } = prepare_run_input(pool, row, req, default_timeout_ms, max_body_bytes).await?;

    if req.mode == "auth_only" {
        validate_auth_only_ssrf(engine.ssrf(), &row.upstream_url, &input).await?;
    }

    let result = engine.invoke(input).await;
    let ep = endpoint_by_id(&row.endpoints, req.endpoint_id.as_deref());
    let captured = extract_endpoint_captures(ep, result.payload.as_ref());
    let assertion_results = run_draft_assertions(req, &row.endpoints, &result);
    let assertions_ok = assertion_results.is_empty() || assertion_results.iter().all(|a| a.ok);
    let overall_ok = result.ok && assertions_ok;
    let stages_json = serde_json::to_value(&result.stages).unwrap_or(serde_json::json!([]));
    let assertions_json = serde_json::to_value(&assertion_results).unwrap_or(serde_json::json!([]));
    let error_code = run_error_code(&result, overall_ok, assertions_ok);
    connector_metrics::record_run(&req.mode, overall_ok, error_code);

    let run_row = repo::insert_run(
        pool,
        repo::NewRun {
            draft_id: id,
            supplier_id: None,
            mode: &req.mode,
            ok: overall_ok,
            latency_ms: Some(i32::saturating_from(result.latency_ms)),
            stages: &stages_json,
            assertions: &assertions_json,
            error_code,
            error_message: result.error_message.as_deref(),
            endpoint_key: endpoint_key.as_deref(),
            testcase_id: req.testcase_id,
            actor: Some(actor),
            spec_fingerprint: None,
            probe_path: None,
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("insert run: {e}")))?;

    audit.log(AuditLogRequest {
        user_id: Some(user_id),
        action: AuditAction::ConnectorRun,
        resource_type: AuditResourceType::Supplier,
        resource_id: id.map(|u| u.to_string()),
        details: Some(serde_json::json!({
            "run_id": run_row.id,
            "mode": req.mode,
            "ok": overall_ok,
            "probe_path": serde_json::Value::Null,
        })),
        ip_address: client_ip,
        ..Default::default()
    });

    Ok(run_dto_from_row(
        run_row,
        run_response_preview(&result, &request_id, &captured),
    ))
}

/// business/live：mgmt → bypass 管理探针 HTTP；写入 `probe_path=bypass_mgmt_probe`。
struct BypassRunParams<'a> {
    audit: &'a AuditService,
    bypass: &'a BypassProbeClient,
    ssrf: &'a SsrfChecker,
    id: Option<Uuid>,
    row: &'a repo::DraftRow,
    req: &'a RunRequest,
    actor: &'a str,
    user_id: Uuid,
    client_ip: Option<String>,
    default_timeout_ms: u64,
    max_body_bytes: usize,
}

async fn run_draft_via_bypass(
    pool: &PgPool,
    params: BypassRunParams<'_>,
) -> Result<RunDto, ApiError> {
    let BypassRunParams {
        audit,
        bypass,
        ssrf,
        id,
        row,
        req,
        actor,
        user_id,
        client_ip,
        default_timeout_ms,
        max_body_bytes,
    } = params;

    let prepared =
        prepare_bypass_probe_from_draft(pool, row, req, default_timeout_ms, max_body_bytes).await?;

    // 双重 SSRF：mgmt 预检 + bypass 进程内再拦。
    ssrf::validate_public_http_url(ssrf, &prepared.upstream_url_for_ssrf)
        .await
        .map_err(|e| ApiError::validation(format!("SSRF_BLOCKED: {e}")))?;
    for token_url in &prepared.token_urls_for_ssrf {
        ssrf::validate_public_http_url(ssrf, token_url)
            .await
            .map_err(|e| ApiError::validation(format!("SSRF_BLOCKED token_url: {e}")))?;
    }

    let invoke_params = ProbeInvokeParams {
        channel_id: &prepared.channel_id,
        request_id: &prepared.request_id,
        timeout_ms: prepared.timeout_ms,
        payload_json: &prepared.payload_json,
        egress_spec_json: &prepared.egress_spec_json,
        supplier_id: &prepared.supplier_id,
        supplier_code: &prepared.supplier_code,
        supplier_config_json: &prepared.supplier_config_json,
    };

    let outcome = match bypass.execute_invoke(&invoke_params).await {
        Ok(resp) => bypass_probe::outcome_from_invoke_response(&resp),
        Err(e) => ProbeOutcome::from_probe_error(&e),
    };

    let ep = endpoint_by_id(&row.endpoints, req.endpoint_id.as_deref());
    let captured = extract_endpoint_captures(ep, outcome.payload.as_ref());
    let assertion_results = run_bypass_assertions(req, &row.endpoints, &outcome);
    let assertions_ok = assertion_results.is_empty() || assertion_results.iter().all(|a| a.ok);
    let overall_ok = outcome.ok && assertions_ok;
    let assertions_json = serde_json::to_value(&assertion_results).unwrap_or(serde_json::json!([]));
    let error_code = if overall_ok {
        None
    } else if !outcome.ok {
        outcome.error_code.as_deref()
    } else {
        Some("ASSERTION_FAILED")
    };
    connector_metrics::record_run(&req.mode, overall_ok, error_code);

    let mut dto = persist_bypass_run(PersistBypassRun {
        pool,
        audit,
        draft_id: id,
        mode: &req.mode,
        overall_ok,
        outcome: &outcome,
        request_id: &prepared.request_id,
        captured: &captured,
        assertions_json: &assertions_json,
        error_code,
        endpoint_key: prepared.endpoint_key.as_deref(),
        testcase_id: req.testcase_id,
        actor,
        user_id,
        client_ip,
        fingerprint: &prepared.fingerprint,
    })
    .await?;

    let extra = if overall_ok {
        vec![RecentGateRun {
            endpoint_key: prepared.endpoint_key.clone(),
            spec_fingerprint: Some(prepared.fingerprint.clone()),
        }]
    } else {
        Vec::new()
    };
    dto.gate = evaluate_gate_items_for_row(pool, id, row, &extra).await?;
    Ok(dto)
}

struct PersistBypassRun<'a> {
    pool: &'a PgPool,
    audit: &'a AuditService,
    draft_id: Option<Uuid>,
    mode: &'a str,
    overall_ok: bool,
    outcome: &'a ProbeOutcome,
    request_id: &'a str,
    captured: &'a BTreeMap<String, String>,
    assertions_json: &'a serde_json::Value,
    error_code: Option<&'a str>,
    endpoint_key: Option<&'a str>,
    testcase_id: Option<Uuid>,
    actor: &'a str,
    user_id: Uuid,
    client_ip: Option<String>,
    fingerprint: &'a str,
}

async fn persist_bypass_run(p: PersistBypassRun<'_>) -> Result<RunDto, ApiError> {
    let run_row = repo::insert_run(
        p.pool,
        repo::NewRun {
            draft_id: p.draft_id,
            supplier_id: None,
            mode: p.mode,
            ok: p.overall_ok,
            latency_ms: Some(i32::saturating_from(p.outcome.upstream_wire_ms)),
            stages: &p.outcome.stages,
            assertions: p.assertions_json,
            error_code: p.error_code,
            error_message: p.outcome.error_message.as_deref(),
            endpoint_key: p.endpoint_key,
            testcase_id: p.testcase_id,
            actor: Some(p.actor),
            spec_fingerprint: if p.overall_ok {
                Some(p.fingerprint)
            } else {
                None
            },
            probe_path: Some(PROBE_PATH_BYPASS_MGMT_PROBE),
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("insert run: {e}")))?;

    p.audit.log(AuditLogRequest {
        user_id: Some(p.user_id),
        action: AuditAction::ConnectorRun,
        resource_type: AuditResourceType::Supplier,
        resource_id: p.draft_id.map(|u| u.to_string()),
        details: Some(serde_json::json!({
            "run_id": run_row.id,
            "mode": p.mode,
            "ok": p.overall_ok,
            "probe_path": PROBE_PATH_BYPASS_MGMT_PROBE,
        })),
        ip_address: p.client_ip,
        ..Default::default()
    });

    Ok(run_dto_from_row(
        run_row,
        bypass_response_preview(p.outcome, p.request_id, p.captured),
    ))
}

fn bypass_response_preview(
    outcome: &ProbeOutcome,
    request_id: &str,
    captured: &BTreeMap<String, String>,
) -> Option<serde_json::Value> {
    let masked_headers = mask_headers(&outcome.headers, true);
    outcome
        .payload
        .as_ref()
        .map(|p| {
            serde_json::json!({
                "status": outcome.status.unwrap_or(if outcome.ok { 200 } else { 400 }),
                "headers": masked_headers,
                "body_preview": p.to_string().chars().take(4096).collect::<String>(),
                "url": outcome.url.clone().unwrap_or_default(),
                "adapter": outcome.adapter,
                "biz_error": outcome.biz_error,
                "request_id": request_id,
                "upstream_request_id": outcome.upstream_request_id,
                "trace_id": outcome.trace_id,
                "captured": captured,
                "probe_path": PROBE_PATH_BYPASS_MGMT_PROBE,
            })
        })
        .or_else(|| {
            Some(serde_json::json!({
                "status": outcome.status,
                "headers": masked_headers,
                "url": outcome.url,
                "adapter": outcome.adapter,
                "error": outcome.error_message,
                "request_id": request_id,
                "upstream_request_id": outcome.upstream_request_id,
                "trace_id": outcome.trace_id,
                "captured": captured,
                "probe_path": PROBE_PATH_BYPASS_MGMT_PROBE,
            }))
        })
}

fn run_bypass_assertions(
    req: &RunRequest,
    endpoints: &serde_json::Value,
    outcome: &ProbeOutcome,
) -> Vec<AssertionResult> {
    if !outcome.ok && outcome.status.is_none() {
        return Vec::new();
    }
    let raw: Vec<serde_json::Value> = if let Some(ref raw_assertions) = req.assertions {
        raw_assertions.clone()
    } else {
        let Some(ep_id) = req.endpoint_id.as_deref() else {
            return Vec::new();
        };
        let Some(arr) = endpoints
            .as_array()
            .and_then(|eps| {
                eps.iter().find(|e| {
                    e.get("id").and_then(|v| v.as_str()) == Some(ep_id)
                        || e.get("name").and_then(|v| v.as_str()) == Some(ep_id)
                })
            })
            .and_then(|ep| ep.get("assertions"))
            .and_then(|v| v.as_array())
        else {
            return Vec::new();
        };
        arr.clone()
    };
    let parsed: Vec<Assertion> = raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    let body = outcome
        .payload
        .as_ref()
        .map_or_default(|p| serde_json::to_vec(p).unwrap_or_default());
    let response = OutboundResponse {
        status: outcome.status.unwrap_or(if outcome.ok { 200 } else { 400 }),
        headers: outcome.headers.clone(),
        body,
        final_url: outcome.url.clone().unwrap_or_default(),
        method: "POST".into(),
        path: "/".into(),
        protocol: String::new(),
        wire_duration: Duration::ZERO,
    };
    evaluate_assertions(&parsed, Some(&response), outcome.upstream_wire_ms)
}

struct PreparedBypassProbe {
    channel_id: String,
    request_id: String,
    timeout_ms: u64,
    payload_json: String,
    egress_spec_json: String,
    supplier_id: String,
    supplier_code: String,
    supplier_config_json: String,
    endpoint_key: Option<String>,
    fingerprint: String,
    upstream_url_for_ssrf: String,
    token_urls_for_ssrf: Vec<String>,
}

/// 草稿物化 egress + 密文 config JSON → bypass Invoke（密钥仅在 bypass 解密）。
async fn prepare_bypass_probe_from_draft(
    pool: &PgPool,
    row: &repo::DraftRow,
    req: &RunRequest,
    default_timeout_ms: u64,
    max_body_bytes: usize,
) -> Result<PreparedBypassProbe, ApiError> {
    let env = parse_workbench_env_with_id(&row.workbench_env, req.env_id.as_deref());
    let mut cfg = SupplierConfig::parse_and_validate(&row.config)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    // 环境变量展开到 config 副本；凭据保持库内密文，bypass 再 decrypt。
    super::workbench_spec::apply_env_to_supplier_config(&mut cfg, &env);
    let supplier_config_json = serde_json::to_string(&cfg)
        .map_err(|e| ApiError::internal(format!("serialize supplier config: {e}")))?;

    let ep = endpoint_by_id(&row.endpoints, req.endpoint_id.as_deref());
    let empty_ep = serde_json::json!({});
    let empty_caps = HashMap::new();
    let caps_ref = req.captures.as_ref().unwrap_or(&empty_caps);
    let built = build_workbench_spec_with_captures(
        ep.unwrap_or(&empty_ep),
        &env,
        Some(caps_ref),
        Some(&cfg),
        &row.upstream_url,
    )?;
    let payload_json = if built.payload.is_null() {
        String::new()
    } else {
        built.payload.to_string()
    };
    if payload_json.len() > max_body_bytes {
        return Err(ApiError::validation(format!(
            "request body exceeds max_body_bytes ({max_body_bytes})"
        )));
    }
    let endpoint_key = ep.and_then(|e| workbench_endpoint_key(e, req.endpoint_id.as_deref()));
    let timeout_ms = built
        .spec
        .timeout_ms
        .or_else(|| cfg.timeouts.as_ref().and_then(|t| t.total_ms))
        .unwrap_or(default_timeout_ms);

    // artifact：bypass 按 artifact_id 自库加载；此处仅确保草稿引用可解析（可选预检）。
    if let Some(ref aid) = built.spec.artifact_id {
        let _ = load_artifact_bytes(pool, aid).await?;
    }

    let egress_spec_json = serde_json::to_string(&built.spec)
        .map_err(|e| ApiError::internal(format!("serialize egress: {e}")))?;
    // 与生产一致：hash 供 bypass 缓存键（full JSON 始终下发，避免 CONFIG_CACHE_MISS）。
    let _egress_hash = hash_egress_spec_json(&egress_spec_json);

    let mut token_urls_for_ssrf = Vec::new();
    if let Some(token_url) = cfg
        .auth_flow
        .as_ref()
        .filter(|a| a.enabled)
        .map(|a| a.token_url.trim())
        .filter(|s| !s.is_empty())
    {
        token_urls_for_ssrf.push(token_url.to_string());
    }
    if let Some(ep_token_url) = built
        .spec
        .auth
        .as_ref()
        .and_then(|a| a.token_url.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        && !token_urls_for_ssrf.iter().any(|u| u == ep_token_url)
    {
        token_urls_for_ssrf.push(ep_token_url.to_string());
    }

    let expanded_url = if built.spec.base_url.trim().is_empty() {
        row.upstream_url.clone()
    } else {
        built.spec.base_url.clone()
    };

    Ok(PreparedBypassProbe {
        channel_id: endpoint_key
            .clone()
            .unwrap_or_else(|| row.id.to_string()),
        request_id: Uuid::new_v4().to_string(),
        timeout_ms,
        payload_json,
        egress_spec_json,
        supplier_id: row.linked_supplier_id.map_or_default(|u| u.to_string()),
        supplier_code: format!("probe:{}", row.draft_code),
        supplier_config_json,
        endpoint_key,
        fingerprint: built.fingerprint,
        upstream_url_for_ssrf: expanded_url,
        token_urls_for_ssrf,
    })
}

const MAX_LOCAL_PROBE_TIMEOUT_MS: u64 = 30_000;

/// 一次草稿试跑的执行输入。
struct PreparedRun {
    input: InvokeInput,
    endpoint_key: Option<String>,
    request_id: String,
}

/// 从已保存草稿构建执行输入（D36：忽略请求体中的 spec/payload）。
async fn prepare_run_input(
    pool: &PgPool,
    row: &repo::DraftRow,
    req: &RunRequest,
    default_timeout_ms: u64,
    max_body_bytes: usize,
) -> Result<PreparedRun, ApiError> {
    let mut cfg = SupplierConfig::parse_and_validate(&row.config)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    cfg = cfg
        .decrypt_credentials_for_runtime()
        .map_err(|e| ApiError::internal(format!("decrypt credentials: {e}")))?;

    // Spec and payload come from the saved draft via the single builder (Tab fields + workbench_env + session captures).
    let env = parse_workbench_env_with_id(&row.workbench_env, req.env_id.as_deref());
    let ep = endpoint_by_id(&row.endpoints, req.endpoint_id.as_deref());
    let empty_ep = serde_json::json!({});
    let empty_caps = HashMap::new();
    let caps_ref = req.captures.as_ref().unwrap_or(&empty_caps);
    let built = build_workbench_spec_with_captures(
        ep.unwrap_or(&empty_ep),
        &env,
        Some(caps_ref),
        Some(&cfg),
        &row.upstream_url,
    )?;
    let payload = built.payload.clone();
    let payload_str = payload.to_string();
    if payload_str.len() > max_body_bytes {
        return Err(ApiError::validation(format!(
            "request body exceeds max_body_bytes ({max_body_bytes})"
        )));
    }
    let endpoint_key = ep.and_then(|e| workbench_endpoint_key(e, req.endpoint_id.as_deref()));
    let spec = built
        .spec
        .decrypt_secrets_for_runtime()
        .map_err(|e| ApiError::internal(format!("decrypt egress secrets: {e}")))?;

    let mode = match req.mode.as_str() {
        "auth_only" => ExecuteMode::AuthOnly,
        "transform_preview" => ExecuteMode::TransformPreview,
        "live" => ExecuteMode::Live,
        _ => ExecuteMode::Business,
    };

    let timeout_ms = spec
        .timeout_ms
        .or_else(|| cfg.timeouts.as_ref().and_then(|t| t.total_ms))
        .unwrap_or(default_timeout_ms)
        .min(MAX_LOCAL_PROBE_TIMEOUT_MS);

    let mut credentials = HashMap::new();
    if let Some(ref auth) = cfg.auth_flow {
        credentials.clone_from(&auth.credentials);
    }

    let artifact_bytes = if let Some(ref aid) = spec.artifact_id {
        load_artifact_bytes(pool, aid).await?
    } else {
        None
    };

    let request_id = Uuid::new_v4().to_string();
    let input = InvokeInput {
        supplier_id: row.linked_supplier_id.map_or_default(|u| u.to_string()),
        supplier_code: format!("probe:{}", row.draft_code),
        channel_id: endpoint_key.clone().unwrap_or_else(|| row.id.to_string()),
        request_id: request_id.clone(),
        payload,
        spec: Arc::new(spec),
        spec_validated: true,
        config: Some(Arc::new(cfg)),
        timeout: Duration::from_millis(timeout_ms),
        mode,
        options: ExecuteOptions {
            max_body_capture_bytes: 64 * 1024,
            mask_secrets: true,
            collect_trace: true,
        },
        pipeline_keys: Some(Arc::new(
            mazbri_upstream_runtime::pipeline_keys_from_credentials(&credentials),
        )),
        credentials,
        artifact_bytes: artifact_bytes.map(Arc::from),
        descriptor_pool: None,
        cache_key: Some(format!("probe:{}", row.id)),
    };

    Ok(PreparedRun {
        input,
        endpoint_key,
        request_id,
    })
}

pub(super) fn run_dto_from_row(
    run_row: repo::RunRow,
    response_preview: Option<serde_json::Value>,
) -> RunDto {
    RunDto {
        id: run_row.id,
        draft_id: run_row.draft_id,
        supplier_id: run_row.supplier_id,
        mode: run_row.mode,
        ok: run_row.ok,
        latency_ms: run_row.latency_ms,
        stages: run_row.stages,
        assertions: run_row.assertions,
        error_code: run_row.error_code,
        error_message: run_row.error_message,
        endpoint_key: run_row.endpoint_key,
        actor: run_row.actor,
        created_at: run_row.created_at,
        response_preview,
        spec_fingerprint: run_row.spec_fingerprint,
        probe_path: run_row.probe_path,
        gate: Vec::new(),
    }
}

/// 校验阶段：微服务草稿必须配置 `contract_id`。
fn probe_contract_stage(cfg: &SupplierConfig) -> (bool, serde_json::Value) {
    let contract_id = cfg.contract_id.clone().unwrap_or_default();
    let contract_ok = !contract_id.trim().is_empty();
    let stage = serde_json::json!({
        "name": "validate",
        "ok": contract_ok,
        "latency_ms": 0,
        "response_summary": {
            "detail": if contract_ok {
                format!("contract_id={contract_id}")
            } else {
                "contract_id missing".into()
            }
        },
        "error": if contract_ok { serde_json::Value::Null } else { serde_json::json!("contract_id required") }
    });
    (contract_ok, stage)
}

fn parse_probe_host_port(upstream_url: &str) -> Result<(String, u16), String> {
    let trimmed = upstream_url.trim();
    if trimmed.is_empty() {
        return Err("empty upstream_url".to_string());
    }
    if trimmed.contains("://") {
        let parsed =
            url::Url::parse(trimmed).map_err(|e| format!("invalid upstream_url: {e}"))?;
        let raw_host = parsed
            .host_str()
            .ok_or_else(|| "missing host in upstream_url".to_string())?;
        let host = raw_host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        let default_port = match parsed.scheme() {
            "http" | "ws" => Some(80),
            "https" | "wss" => Some(443),
            _ => None,
        };
        let port = parsed
            .port()
            .or(default_port)
            .ok_or_else(|| "missing explicit port in upstream_url".to_string())?;
        if host.is_empty() || port == 0 {
            return Err("invalid host or port".to_string());
        }
        return Ok((host, port));
    }

    let authority = trimmed.split('/').next().unwrap_or("").trim();
    if authority.is_empty() {
        return Err("empty host:port".to_string());
    }
    let (host, port_str) = if let Some(rest) = authority.strip_prefix('[') {
        rest.split_once("]:")
            .ok_or_else(|| "invalid IPv6 [host]:port".to_string())?
    } else {
        authority
            .rsplit_once(':')
            .ok_or_else(|| "missing port in host:port".to_string())?
    };
    let port: u16 = port_str
        .parse()
        .map_err(|_| "invalid port in host:port".to_string())?;
    if host.trim().is_empty() || port == 0 {
        return Err("invalid host or port".to_string());
    }
    Ok((host.trim().to_string(), port))
}

/// 连通性阶段：先经 `SsrfChecker::resolve_checked_socket_addrs` 核验并固定已审 `SocketAddr`，再执行 TCP 连接（消除二次 DNS 解析 TOCTOU）。
async fn probe_connectivity_stage(
    ssrf: &SsrfChecker,
    upstream_url: &str,
    default_timeout_ms: u64,
) -> (bool, serde_json::Value) {
    let c_start = std::time::Instant::now();
    let display_target = upstream_url.trim().to_string();
    let (connect_ok, err_msg) = match parse_probe_host_port(upstream_url) {
        Err(parse_err) => (false, Some(format!("invalid probe target: {parse_err}"))),
        Ok((host, port)) => match ssrf.resolve_checked_socket_addrs(&host, port).await {
            Err(ssrf_err) => (false, Some(format!("SSRF_BLOCKED: {ssrf_err}"))),
            Ok(addrs) if addrs.is_empty() => (false, Some("DNS resolved empty address list".into())),
            Ok(addrs) => {
                let timeout = Duration::from_millis(default_timeout_ms.min(5_000));
                if matches!(
                    tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addrs[..])).await,
                    Ok(Ok(_))
                ) {
                    (true, None)
                } else {
                    (false, Some("TCP connect failed".into()))
                }
            }
        },
    };
    let stage = serde_json::json!({
        "name": "probe_connectivity",
        "ok": connect_ok,
        "latency_ms": mazbri_core::num::millis_u64(c_start.elapsed()),
        "request_summary": { "url": display_target },
        "error": match err_msg {
            None => serde_json::Value::Null,
            Some(msg) => serde_json::Value::String(msg),
        }
    });
    (connect_ok, stage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_probe_host_port_handles_schemes_and_ipv6() {
        assert_eq!(
            parse_probe_host_port("grpc://svc.example.com:50051/pkg.Svc").expect("grpc url"),
            ("svc.example.com".to_string(), 50051)
        );
        assert_eq!(
            parse_probe_host_port("https://api.example.com/health").expect("https default 443"),
            ("api.example.com".to_string(), 443)
        );
        assert_eq!(
            parse_probe_host_port("svc.example.com:9090").expect("bare host:port"),
            ("svc.example.com".to_string(), 9090)
        );
        assert_eq!(
            parse_probe_host_port("[2001:db8::1]:8443").expect("ipv6 host:port"),
            ("2001:db8::1".to_string(), 8443)
        );
        assert!(parse_probe_host_port("grpc://svc.example.com").is_err());
    }

    #[tokio::test]
    async fn probe_connectivity_stage_blocks_loopback_via_ssrf() {
        let checker = SsrfChecker::new();
        let (ok, stage) = probe_connectivity_stage(&checker, "127.0.0.1:8080", 1_000).await;
        assert!(!ok);
        let err = stage["error"].as_str().unwrap_or("");
        assert!(err.contains("SSRF_BLOCKED"), "unexpected error: {err}");
    }
}
