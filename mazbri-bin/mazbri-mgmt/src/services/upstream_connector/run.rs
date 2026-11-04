use super::{
    ApiError, Arc, Assertion, AssertionResult, AuditAction, AuditLogRequest, AuditResourceType,
    AuditService, DEFAULT_RUN_LIMIT, Duration, ExecuteMode, ExecuteOptions, HashMap, InvokeInput,
    OutboundResponse, PgPool, RunDto, RunRequest, RuntimeEngine, SsrfChecker, SupplierConfig,
    UPSTREAM_TYPE_TRANSPARENT, Uuid, build_workbench_spec, connector_metrics, endpoint_by_id,
    evaluate_assertions, load_artifact_bytes, mask_config_credentials, parse_workbench_env, repo,
    ssrf, workbench_endpoint_key,
};
use crate::services::bypass_probe::{
    self, BypassProbeClient, PROBE_PATH_BYPASS_MGMT_PROBE, ProbeInvokeParams, ProbeOutcome,
};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::hash_egress_spec_json;

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
    pub id: Uuid,
    pub req: RunRequest,
    pub actor: &'a str,
    pub user_id: Uuid,
    pub client_ip: Option<String>,
    pub default_timeout_ms: u64,
    pub max_body_bytes: usize,
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
        client_ip,
        default_timeout_ms,
        max_body_bytes,
    } = params;
    let row = repo::get_draft(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("get draft: {e}")))?
        .ok_or_else(|| ApiError::not_found("draft".into(), id.to_string()))?;

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
        probe_connectivity_stage(&row.upstream_url, default_timeout_ms).await;
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
            draft_id: Some(row.id),
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
        resource_id: Some(row.id.to_string()),
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
        draft_id: Some(row.id),
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

fn run_response_preview(
    result: &mazbri_upstream_runtime::InvokeResult,
) -> Option<serde_json::Value> {
    result.payload.as_ref().map(|p| {
        let status = result
            .upstream_meta
            .as_ref()
            .map_or(if result.ok { 200 } else { 400 }, |m| m.status);
        serde_json::json!({
            "status": status,
            "body_preview": p.to_string().chars().take(4096).collect::<String>(),
            "url": result.upstream_meta.as_ref().map(|m| m.url.clone()).unwrap_or_default(),
            "adapter": result.upstream_meta.as_ref().map(|m| m.adapter.clone()),
            "biz_error": result.biz_error,
        })
    }).or_else(|| {
        result.response.as_ref().map(|r| {
            serde_json::json!({
                "status": r.status,
                "headers": r.headers,
                "body_preview": String::from_utf8_lossy(&r.body).chars().take(4096).collect::<String>(),
                "url": r.final_url,
            })
        })
    })
}


/// `auth_only` / `transform_preview`：mgmt 本地 `RuntimeEngine`；`probe_path` 为 `NULL`，永不进 gate。
struct LocalNonGateParams<'a> {
    audit: &'a AuditService,
    engine: &'a RuntimeEngine,
    id: Uuid,
    row: &'a repo::DraftRow,
    req: &'a RunRequest,
    actor: &'a str,
    user_id: Uuid,
    client_ip: Option<String>,
    default_timeout_ms: u64,
    max_body_bytes: usize,
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
    } = prepare_run_input(pool, row, req, default_timeout_ms, max_body_bytes).await?;

    let result = engine.invoke(input).await;
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
            draft_id: Some(id),
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
        resource_id: Some(id.to_string()),
        details: Some(serde_json::json!({
            "run_id": run_row.id,
            "mode": req.mode,
            "ok": overall_ok,
            "probe_path": serde_json::Value::Null,
        })),
        ip_address: client_ip,
        ..Default::default()
    });

    Ok(run_dto_from_row(run_row, run_response_preview(&result)))
}

/// business/live：mgmt → bypass 管理探针 HTTP；写入 `probe_path=bypass_mgmt_probe`。
struct BypassRunParams<'a> {
    audit: &'a AuditService,
    bypass: &'a BypassProbeClient,
    ssrf: &'a SsrfChecker,
    id: Uuid,
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
    if let Some(ref token_url) = prepared.token_url_for_ssrf {
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
        Err(e) => ProbeOutcome {
            ok: false,
            latency_ms: 0,
            error_code: Some(classify_transport_err(&e)),
            error_message: Some(e),
            payload: None,
            status: None,
            url: None,
            adapter: None,
            biz_error: None,
            stages: serde_json::json!([{
                "name": "bypass_mgmt_probe",
                "ok": false,
                "error": "transport",
                "response_summary": {"probe_path": PROBE_PATH_BYPASS_MGMT_PROBE},
            }]),
        },
    };

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

    persist_bypass_run(PersistBypassRun {
        pool,
        audit,
        draft_id: id,
        mode: &req.mode,
        overall_ok,
        outcome: &outcome,
        assertions_json: &assertions_json,
        error_code,
        endpoint_key: prepared.endpoint_key.as_deref(),
        testcase_id: req.testcase_id,
        actor,
        user_id,
        client_ip,
        fingerprint: &prepared.fingerprint,
    })
    .await
}

struct PersistBypassRun<'a> {
    pool: &'a PgPool,
    audit: &'a AuditService,
    draft_id: Uuid,
    mode: &'a str,
    overall_ok: bool,
    outcome: &'a ProbeOutcome,
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
            draft_id: Some(p.draft_id),
            supplier_id: None,
            mode: p.mode,
            ok: p.overall_ok,
            latency_ms: Some(i32::saturating_from(p.outcome.latency_ms)),
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
        resource_id: Some(p.draft_id.to_string()),
        details: Some(serde_json::json!({
            "run_id": run_row.id,
            "mode": p.mode,
            "ok": p.overall_ok,
            "probe_path": PROBE_PATH_BYPASS_MGMT_PROBE,
        })),
        ip_address: p.client_ip,
        ..Default::default()
    });

    Ok(run_dto_from_row(run_row, bypass_response_preview(p.outcome)))
}

fn classify_transport_err(err: &str) -> String {
    if err.contains("BYPASS_PROBE_HTTP") || err.contains("BYPASS_NOT_READY") {
        "BYPASS_UNAVAILABLE".into()
    } else {
        "BYPASS_TRANSPORT".into()
    }
}

fn bypass_response_preview(outcome: &ProbeOutcome) -> Option<serde_json::Value> {
    outcome.payload.as_ref().map(|p| {
        serde_json::json!({
            "status": outcome.status.unwrap_or(if outcome.ok { 200 } else { 400 }),
            "body_preview": p.to_string().chars().take(4096).collect::<String>(),
            "url": outcome.url.clone().unwrap_or_default(),
            "adapter": outcome.adapter,
            "biz_error": outcome.biz_error,
            "probe_path": PROBE_PATH_BYPASS_MGMT_PROBE,
        })
    }).or_else(|| {
        Some(serde_json::json!({
            "status": outcome.status,
            "url": outcome.url,
            "adapter": outcome.adapter,
            "error": outcome.error_message,
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
        .map(|p| serde_json::to_vec(p).unwrap_or_default())
        .unwrap_or_default();
    let response = OutboundResponse {
        status: outcome.status.unwrap_or(if outcome.ok { 200 } else { 400 }),
        headers: HashMap::default(),
        body,
        final_url: outcome.url.clone().unwrap_or_default(),
        method: "POST".into(),
        path: "/".into(),
    };
    evaluate_assertions(&parsed, Some(&response), outcome.latency_ms)
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
    token_url_for_ssrf: Option<String>,
}

/// 草稿物化 egress + 密文 config JSON → bypass Invoke（密钥仅在 bypass 解密）。
async fn prepare_bypass_probe_from_draft(
    pool: &PgPool,
    row: &repo::DraftRow,
    req: &RunRequest,
    default_timeout_ms: u64,
    max_body_bytes: usize,
) -> Result<PreparedBypassProbe, ApiError> {
    let env = parse_workbench_env(&row.workbench_env);
    let mut cfg = SupplierConfig::parse_and_validate(&row.config)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    // 环境变量展开到 config 副本；凭据保持库内密文，bypass 再 decrypt。
    super::workbench_spec::apply_env_to_supplier_config(&mut cfg, &env);
    let supplier_config_json = serde_json::to_string(&cfg)
        .map_err(|e| ApiError::internal(format!("serialize supplier config: {e}")))?;

    let ep = endpoint_by_id(&row.endpoints, req.endpoint_id.as_deref());
    let empty_ep = serde_json::json!({});
    let built = build_workbench_spec(ep.unwrap_or(&empty_ep), &env, Some(&cfg), &row.upstream_url)?;
    let payload_json = built.payload.to_string();
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

    let token_url_for_ssrf = cfg
        .auth_flow
        .as_ref()
        .filter(|a| a.enabled)
        .map(|a| a.token_url.clone());

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
        supplier_id: row
            .linked_supplier_id
            .map(|u| u.to_string())
            .unwrap_or_default(),
        supplier_code: format!("probe:{}", row.draft_code),
        supplier_config_json,
        endpoint_key,
        fingerprint: built.fingerprint,
        upstream_url_for_ssrf: expanded_url,
        token_url_for_ssrf,
    })
}


/// 一次草稿试跑的执行输入。
struct PreparedRun {
    input: InvokeInput,
    endpoint_key: Option<String>,
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

    // D36: ignore the entire RunRequestBody. Spec and payload come from the
    // saved draft via the single builder (Tab fields + workbench_env).
    let env = parse_workbench_env(&row.workbench_env);
    let ep = endpoint_by_id(&row.endpoints, req.endpoint_id.as_deref());
    let empty_ep = serde_json::json!({});
    let built = build_workbench_spec(ep.unwrap_or(&empty_ep), &env, Some(&cfg), &row.upstream_url)?;
    let payload = built.payload.clone();
    let payload_str = payload.to_string();
    if payload_str.len() > max_body_bytes {
        return Err(ApiError::validation(format!(
            "request body exceeds max_body_bytes ({max_body_bytes})"
        )));
    }
    let endpoint_key = ep.and_then(|e| workbench_endpoint_key(e, req.endpoint_id.as_deref()));
    let spec = built.spec.clone();

    let mode = match req.mode.as_str() {
        "auth_only" => ExecuteMode::AuthOnly,
        "transform_preview" => ExecuteMode::TransformPreview,
        "live" => ExecuteMode::Live,
        _ => ExecuteMode::Business,
    };

    let timeout_ms = spec
        .timeout_ms
        .or_else(|| cfg.timeouts.as_ref().and_then(|t| t.total_ms))
        .unwrap_or(default_timeout_ms);

    let mut credentials = HashMap::new();
    if let Some(ref auth) = cfg.auth_flow {
        credentials.clone_from(&auth.credentials);
    }

    let artifact_bytes = if let Some(ref aid) = spec.artifact_id {
        load_artifact_bytes(pool, aid).await?
    } else {
        None
    };

    let input = InvokeInput {
        supplier_id: row
            .linked_supplier_id
            .map(|u| u.to_string())
            .unwrap_or_default(),
        supplier_code: format!("probe:{}", row.draft_code),
        channel_id: endpoint_key.clone().unwrap_or_else(|| row.id.to_string()),
        request_id: Uuid::new_v4().to_string(),
        payload,
        spec,
        config: Some(Arc::new(cfg)),
        timeout: Duration::from_millis(timeout_ms),
        mode,
        options: ExecuteOptions {
            follow_redirects: false,
            max_body_capture_bytes: 64 * 1024,
            mask_secrets: true,
            collect_trace: true,
        },
        pipeline_keys: Some(mazbri_upstream_runtime::pipeline_keys_from_credentials(&credentials)),
        credentials,
        artifact_bytes,
        cache_key: Some(format!("probe:{}", row.id)),
    };

    Ok(PreparedRun {
        input,
        endpoint_key,
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

/// 连通性阶段：TCP 连接 host:port（去掉 scheme 与路径）。
async fn probe_connectivity_stage(
    upstream_url: &str,
    default_timeout_ms: u64,
) -> (bool, serde_json::Value) {
    let host_port = upstream_url
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or("")
        .to_string();
    let c_start = std::time::Instant::now();
    let connect_ok = if host_port.is_empty() {
        false
    } else {
        let timeout = Duration::from_millis(default_timeout_ms.min(5_000));
        matches!(
            tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&host_port)).await,
            Ok(Ok(_))
        )
    };
    let stage = serde_json::json!({
        "name": "probe_connectivity",
        "ok": connect_ok,
        "latency_ms": mazbri_core::num::millis_u64(c_start.elapsed()),
        "request_summary": { "url": host_port },
        "error": if connect_ok { serde_json::Value::Null } else { serde_json::json!("TCP connect failed") }
    });
    (connect_ok, stage)
}
