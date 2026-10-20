use super::{PgPool, CreateDraftRequest, DraftDto, ApiError, strip_workbench_env_from_config, repo, draft_to_dto, Uuid, UpdateDraftRequest, SupplierConfig, parse_workbench_env, GATE_TTL_HOURS, RecentGateRun, evaluate_endpoint_gates};

// ---------- Draft CRUD ----------

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_draft(
    pool: &PgPool,
    req: CreateDraftRequest,
    actor: &str,
) -> Result<DraftDto, ApiError> {
    let code = req.draft_code.trim();
    if code.is_empty() || code.len() > 100 {
        return Err(ApiError::validation("draft_code 长度必须在 1..=100"));
    }
    let name = req.name.trim();
    if name.is_empty() {
        return Err(ApiError::validation("name 不能为空"));
    }

    let mut config = req.config.unwrap_or_else(|| serde_json::json!({}));
    strip_workbench_env_from_config(&mut config);
    let endpoints = req.endpoints.unwrap_or_else(|| serde_json::json!([]));
    let workbench_env = req.workbench_env.unwrap_or_else(|| serde_json::json!({}));
    let upstream_url = req.upstream_url.clone();
    let upstream_type = req.upstream_type.clone();
    let source = req.source.unwrap_or_else(|| "manual".into());

    if let Some(obj) = config.as_object_mut() {
        obj.entry("upstream_type")
            .or_insert_with(|| serde_json::Value::String(upstream_type.clone()));
    }

    let config_value = config_json_for_draft(&config, None)?;

    let row = repo::insert_draft(
        pool,
        repo::NewDraft {
            draft_code: code,
            name,
            description: req.description.as_deref(),
            upstream_type: &upstream_type,
            upstream_url: &upstream_url,
            config: &config_value,
            endpoints: &endpoints,
            workbench_env: &workbench_env,
            source: &source,
            linked_supplier_id: None,
            created_by: Some(actor),
        },
    )
    .await
    .map_err(|e| {
        if e.to_string().contains("unique") || e.to_string().contains("duplicate") {
            ApiError::validation(format!("draft_code '{code}' 已存在"))
        } else {
            ApiError::internal(format!("创建 draft 失败: {e}"))
        }
    })?;

    with_gate(pool, row).await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_drafts(
    pool: &PgPool,
    status: Option<&str>,
    q: Option<&str>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<serde_json::Value, ApiError> {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).clamp(1, 100);
    let offset = i64::from(page.saturating_sub(1)) * i64::from(page_size);
    let search_pattern = q
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));
    let search_ref = search_pattern.as_deref();
    let total = repo::count_drafts(pool, status, search_ref)
        .await
        .map_err(|e| ApiError::internal(format!("count drafts: {e}")))?;
    let rows = repo::list_drafts(pool, status, search_ref, i64::from(page_size), offset)
        .await
        .map_err(|e| ApiError::internal(format!("list drafts: {e}")))?;
    let total_pages = mazbri_core::num::f64_to_u64((mazbri_core::num::i64_to_f64(total) / f64::from(page_size)).ceil());
    Ok(serde_json::json!({
        "drafts": rows.into_iter().map(draft_to_dto).collect::<Vec<_>>(),
        "pagination": {
            "page": page,
            "page_size": page_size,
            "total": total,
            "total_pages": total_pages,
        }
    }))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_draft(pool: &PgPool, id: Uuid) -> Result<DraftDto, ApiError> {
    let row = repo::get_draft(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("get draft: {e}")))?
        .ok_or_else(|| ApiError::not_found("draft".into(), id.to_string()))?;
    with_gate(pool, row).await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_draft(
    pool: &PgPool,
    id: Uuid,
    req: UpdateDraftRequest,
) -> Result<DraftDto, ApiError> {
    let existing = repo::get_draft(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("get draft: {e}")))?
        .ok_or_else(|| ApiError::not_found("draft".into(), id.to_string()))?;

    let mut config_opt = None;
    if let Some(ref raw) = req.config {
        let mut raw = raw.clone();
        strip_workbench_env_from_config(&mut raw);
        config_opt = Some(config_json_for_draft(&raw, Some(&existing.config))?);
    }

    let row = repo::update_draft(
        pool,
        repo::DraftUpdate {
            id,
            name: req.name.as_deref(),
            description: req.description.as_deref(),
            upstream_type: req.upstream_type.as_deref(),
            upstream_url: req.upstream_url.as_deref(),
            config: config_opt.as_ref(),
            endpoints: req.endpoints.as_ref(),
            workbench_env: req.workbench_env.as_ref(),
            status: req.status.as_deref(),
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("update draft: {e}")))?
    .ok_or_else(|| ApiError::not_found("draft".into(), id.to_string()))?;

    with_gate(pool, row).await
}

/// Persist workbench JSON without publish-time semantic checks (`token_url`, SSRF, …).
fn config_json_for_draft(
    raw: &serde_json::Value,
    existing: Option<&serde_json::Value>,
) -> Result<serde_json::Value, ApiError> {
    let mut cfg = SupplierConfig::parse_for_draft_write(raw)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    cfg = cfg.normalize_disabled_sections();
    if let (Some(new_auth), Some(old_auth)) = (
        cfg.auth_flow.as_mut(),
        existing
            .and_then(|v| v.get("auth_flow"))
            .and_then(|v| serde_json::from_value::<mazbri_core::AuthFlowConfig>(v.clone()).ok()),
    ) {
        for (k, v) in &mut new_auth.credentials {
            if v.contains('•') || v.is_empty() {
                if let Some(old_v) = old_auth.credentials.get(k) {
                    v.clone_from(old_v);
                }
            }
        }
    }
    cfg.encrypt_credentials_for_storage()
        .map_err(|e| ApiError::internal(format!("encrypt credentials: {e}")))?
        .to_json_value()
        .map_err(|e| ApiError::validation(e.to_string()))
}

async fn with_gate(pool: &PgPool, row: repo::DraftRow) -> Result<DraftDto, ApiError> {
    let env = parse_workbench_env(&row.workbench_env);
    let cfg = SupplierConfig::parse_and_validate_lenient(&row.config).ok();
    let recent = repo::list_recent_gate_runs(pool, row.id, GATE_TTL_HOURS)
        .await
        .map_err(|e| ApiError::internal(format!("gate runs: {e}")))?;
    let recent: Vec<RecentGateRun> = recent
        .into_iter()
        .map(|r| RecentGateRun {
            endpoint_key: r.endpoint_key,
            spec_fingerprint: r.spec_fingerprint,
        })
        .collect();
    let eval = evaluate_endpoint_gates(
        &row.endpoints,
        &env,
        cfg.as_ref(),
        &row.upstream_url,
        &recent,
    );
    let mut dto = draft_to_dto(row);
    dto.gate = eval.items;
    Ok(dto)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_draft(pool: &PgPool, id: Uuid) -> Result<(), ApiError> {
    let n = repo::delete_draft(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("delete draft: {e}")))?;
    if n == 0 {
        return Err(ApiError::not_found("draft".into(), id.to_string()));
    }
    Ok(())
}
