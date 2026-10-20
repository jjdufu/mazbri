use super::{
    ApiError, Arc, Assertion, AssertionResult, AuditAction, AuditLogRequest, AuditResourceType,
    AuditService, DEFAULT_RUN_LIMIT, Duration, ExecuteMode, ExecuteOptions, HashMap, InvokeInput,
    OutboundResponse, PgPool, RunDto, RunRequest, SupplierConfig, UPSTREAM_TYPE_TRANSPARENT, Uuid,
    build_workbench_spec, connector_metrics, endpoint_by_id, evaluate_assertions,
    load_artifact_bytes, mask_config_credentials, parse_workbench_env, repo, runtime_engine, ssrf,
    workbench_endpoint_key,
};
use mazbri_core::num::SaturatingFrom;

// ---------- Validate / Run ----------

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn validate_draft(pool: &PgPool, id: Uuid) -> Result<serde_json::Value, ApiError> {
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
        ssrf::validate_public_http_url(&row.upstream_url)
            .await
            .map_err(|e| ApiError::validation(format!("SSRF_BLOCKED: {e}")))?;
        if let Some(ref auth) = cfg.auth_flow {
            if auth.enabled {
                ssrf::validate_public_http_url(&auth.token_url)
                    .await
                    .map_err(|e| ApiError::validation(format!("SSRF_BLOCKED token_url: {e}")))?;
            }
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

    let mut cfg = SupplierConfig::parse_and_validate_lenient(&row.config)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    cfg = cfg
        .decrypt_credentials_for_runtime()
        .map_err(|e| ApiError::internal(format!("decrypt credentials: {e}")))?;

    // D36: ignore the entire RunRequestBody. Spec and payload come from the
    // saved draft via the single builder (Tab fields + workbench_env).
    let env = parse_workbench_env(&row.workbench_env);
    let ep = endpoint_by_id(&row.endpoints, req.endpoint_id.as_deref());
    let path_prefix = ep
        .and_then(|e| e.get("path_prefix").and_then(|v| v.as_str()))
        .or_else(|| ep.and_then(|e| e.get("path").and_then(|v| v.as_str())))
        .unwrap_or("/");
    let empty_ep = serde_json::json!({});
    let built = build_workbench_spec(
        ep.unwrap_or(&empty_ep),
        &env,
        Some(&cfg),
        path_prefix,
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
        credentials = auth.credentials.clone();
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
        fallback_base_url: built.upstream_url.clone(),
        path_prefix: path_prefix.to_string(),
        timeout: Duration::from_millis(timeout_ms),
        mode,
        options: ExecuteOptions {
            follow_redirects: false,
            max_body_capture_bytes: 64 * 1024,
            mask_secrets: true,
            collect_trace: true,
        },
        credentials,
        artifact_bytes,
        cache_key: Some(format!("probe:{}", row.id)),
    };

    let result = runtime_engine().invoke(input).await;

    let assert_response = invoke_assert_response(&result);
    let has_http_status = result
        .upstream_meta
        .as_ref()
        .map(|m| m.status)
        .or_else(|| result.response.as_ref().map(|r| r.status))
        .is_some();
    let skip_assertions = !result.ok && !has_http_status;
    let mut assertion_results: Vec<AssertionResult> = Vec::new();
    if !skip_assertions {
        if let Some(ref raw_assertions) = req.assertions {
            let parsed: Vec<Assertion> = raw_assertions
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            assertion_results =
                evaluate_assertions(&parsed, assert_response.as_ref(), result.latency_ms);
        } else if let Some(ref ep_id) = req.endpoint_id {
            if let Some(eps) = row.endpoints.as_array() {
                if let Some(ep) = eps.iter().find(|e| {
                    e.get("id").and_then(|v| v.as_str()) == Some(ep_id.as_str())
                        || e.get("name").and_then(|v| v.as_str()) == Some(ep_id.as_str())
                }) {
                    if let Some(arr) = ep.get("assertions").and_then(|v| v.as_array()) {
                        let parsed: Vec<Assertion> = arr
                            .iter()
                            .filter_map(|v| serde_json::from_value(v.clone()).ok())
                            .collect();
                        assertion_results = evaluate_assertions(
                            &parsed,
                            assert_response.as_ref(),
                            result.latency_ms,
                        );
                    }
                }
            }
        }
    }

    let assertions_ok = assertion_results.is_empty() || assertion_results.iter().all(|a| a.ok);
    let overall_ok = result.ok && assertions_ok;

    let stages_json = serde_json::to_value(&result.stages).unwrap_or(serde_json::json!([]));
    let assertions_json = serde_json::to_value(&assertion_results).unwrap_or(serde_json::json!([]));

    let error_code = if overall_ok {
        None
    } else if !result.ok {
        result.error_code.as_deref()
    } else if !assertions_ok {
        Some("ASSERTION_FAILED")
    } else {
        None
    };

    connector_metrics::record_run(&req.mode, overall_ok, error_code);

    let write_fp = overall_ok && matches!(req.mode.as_str(), "business" | "live");
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
            spec_fingerprint: if write_fp {
                Some(built.fingerprint.as_str())
            } else {
                None
            },
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("insert run: {e}")))?;

    audit.log(AuditLogRequest {
        user_id,
        action: AuditAction::ConnectorRun,
        resource_type: AuditResourceType::Supplier,
        resource_id: Some(id.to_string()),
        details: Some(serde_json::json!({
            "run_id": run_row.id,
            "mode": req.mode,
            "ok": overall_ok,
        })),
        ip_address: client_ip,
        ..Default::default()
    });

    let response_preview = result.payload.as_ref().map(|p| {
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
    });

    Ok(RunDto {
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
    })
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
    let mut stages = Vec::new();

    // Stage validate
    let cfg = SupplierConfig::parse_and_validate_lenient(&row.config)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    let contract_id = cfg.contract_id.clone().unwrap_or_default();
    let contract_ok = !contract_id.trim().is_empty();
    stages.push(serde_json::json!({
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
    }));

    // Stage connectivity: TCP connect to host:port (strip scheme if present)
    let target = row.upstream_url.trim().to_string();
    let host_port = target
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
    stages.push(serde_json::json!({
        "name": "probe_connectivity",
        "ok": connect_ok,
        "latency_ms": mazbri_core::num::millis_u64(c_start.elapsed()),
        "request_summary": { "url": host_port },
        "error": if connect_ok { serde_json::Value::Null } else { serde_json::json!("TCP connect failed") }
    }));

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
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("insert run: {e}")))?;

    audit.log(AuditLogRequest {
        user_id,
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
    })
}
