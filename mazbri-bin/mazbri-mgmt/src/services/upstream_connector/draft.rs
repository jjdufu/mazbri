use super::{
    ApiError, CreateDraftRequest, DraftDto, DraftGateItem, DraftSnapshot, GATE_TTL_HOURS, PgPool,
    RecentGateRun, SupplierConfig, UpdateDraftRequest, Uuid, draft_to_dto,
    encode_workbench_env_for_db, evaluate_endpoint_gates, guard_draft_origin_pinning,
    guard_supplier_tls_privilege, parse_workbench_env, reject_client_enc_in_endpoints,
    reject_client_enc_in_supplier_config, reject_client_enc_in_workbench_env, repo,
    sanitize_and_encrypt_endpoints_tls, strip_workbench_env_from_config, validate_workbench_env,
};

// ---------- Draft CRUD ----------

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_draft(
    pool: &PgPool,
    req: CreateDraftRequest,
    actor: &str,
    is_super_admin: bool,
) -> Result<DraftDto, ApiError> {
    create_draft_inner(pool, req, actor, is_super_admin, false).await
}

pub(super) async fn create_draft_from_internal_supplier(
    pool: &PgPool,
    req: CreateDraftRequest,
    actor: &str,
    is_super_admin: bool,
) -> Result<DraftDto, ApiError> {
    create_draft_inner(pool, req, actor, is_super_admin, true).await
}

async fn create_draft_inner(
    pool: &PgPool,
    req: CreateDraftRequest,
    actor: &str,
    is_super_admin: bool,
    allow_internal_enc: bool,
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
    if !allow_internal_enc {
        reject_client_enc_in_supplier_config(&config)?;
    }
    guard_supplier_tls_privilege(&config, None, is_super_admin)?;
    let raw_endpoints = req.endpoints.unwrap_or_else(|| serde_json::json!([]));
    if !allow_internal_enc {
        reject_client_enc_in_endpoints(&raw_endpoints)?;
    }
    let endpoints = sanitize_and_encrypt_endpoints_tls(&raw_endpoints, None, is_super_admin)?;
    let workbench_env = req.workbench_env.unwrap_or_else(|| serde_json::json!([]));
    if !allow_internal_enc {
        reject_client_enc_in_workbench_env(&workbench_env)?;
    }
    validate_workbench_env(&workbench_env)?;
    let db_workbench_env = encode_workbench_env_for_db(&workbench_env);
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
            workbench_env: &db_workbench_env,
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

    if !req.claim_run_ids.is_empty() {
        let _ = repo::claim_ephemeral_runs(pool, row.id, &req.claim_run_ids, actor)
            .await
            .map_err(|e| ApiError::internal(format!("claim runs: {e}")))?;
    }

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
        .map(mazbri_core::config::contains_ilike_pattern);
    let search_ref = search_pattern.as_deref();
    let total = repo::count_drafts(pool, status, search_ref)
        .await
        .map_err(|e| ApiError::internal(format!("count drafts: {e}")))?;
    let rows = repo::list_drafts(pool, status, search_ref, i64::from(page_size), offset)
        .await
        .map_err(|e| ApiError::internal(format!("list drafts: {e}")))?;
    let total_pages = mazbri_core::num::f64_to_u64(
        (mazbri_core::num::i64_to_f64(total) / f64::from(page_size)).ceil(),
    );
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
    is_super_admin: bool,
) -> Result<DraftDto, ApiError> {
    if let Some(ref raw) = req.config {
        reject_client_enc_in_supplier_config(raw)?;
    }
    if let Some(ref raw_eps) = req.endpoints {
        reject_client_enc_in_endpoints(raw_eps)?;
    }
    if let Some(ref env) = req.workbench_env {
        reject_client_enc_in_workbench_env(env)?;
    }

    let existing = repo::get_draft(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("get draft: {e}")))?
        .ok_or_else(|| ApiError::not_found("draft".into(), id.to_string()))?;

    guard_draft_origin_pinning(
        req.upstream_url.as_deref(),
        req.config.as_ref(),
        req.endpoints.as_ref(),
        &existing.upstream_url,
        &existing.config,
        &existing.endpoints,
    )?;

    let db_workbench_env = if let Some(env) = req.workbench_env.as_ref() {
        validate_workbench_env(env)?;
        Some(encode_workbench_env_for_db(env))
    } else {
        None
    };
    let mut config_opt = None;
    if let Some(ref raw) = req.config {
        let mut raw = raw.clone();
        strip_workbench_env_from_config(&mut raw);
        guard_supplier_tls_privilege(&raw, Some(&existing.config), is_super_admin)?;
        config_opt = Some(config_json_for_draft(&raw, Some(&existing.config))?);
    }
    let endpoints_opt = match req.endpoints.as_ref() {
        Some(raw_eps) => Some(sanitize_and_encrypt_endpoints_tls(
            raw_eps,
            Some(&existing.endpoints),
            is_super_admin,
        )?),
        None => None,
    };

    let row = repo::update_draft(
        pool,
        repo::DraftUpdate {
            id,
            name: req.name.as_deref(),
            description: req.description.as_deref(),
            upstream_type: req.upstream_type.as_deref(),
            upstream_url: req.upstream_url.as_deref(),
            config: config_opt.as_ref(),
            endpoints: endpoints_opt.as_ref(),
            workbench_env: db_workbench_env.as_ref(),
            status: req.status.as_deref(),
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("update draft: {e}")))?
    .ok_or_else(|| ApiError::not_found("draft".into(), id.to_string()))?;

    with_gate(pool, row).await
}

/// Persist workbench JSON without publish-time semantic checks (`token_url`, SSRF, …).
pub(super) fn config_json_for_draft(
    raw: &serde_json::Value,
    existing: Option<&serde_json::Value>,
) -> Result<serde_json::Value, ApiError> {
    let mut raw_merged = raw.clone();
    if let Some(old) = existing {
        mazbri_core::merge_auth_credentials(&mut raw_merged, old);
    }
    let mut cfg = SupplierConfig::parse_for_draft_write(&raw_merged)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    cfg = cfg.normalize_disabled_sections();
    if let (Some(new_auth), Some(old_auth)) = (
        cfg.auth_flow.as_mut(),
        existing
            .and_then(|v| v.get("auth_flow"))
            .and_then(|v| serde_json::from_value::<mazbri_core::AuthFlowConfig>(v.clone()).ok()),
    ) {
        for (k, v) in &mut new_auth.credentials {
            if (v.contains('•') || v.is_empty())
                && let Some(old_v) = old_auth.credentials.get(k)
            {
                v.clone_from(old_v);
            }
        }
    }
    cfg.encrypt_credentials_for_storage()
        .map_err(|e| ApiError::internal(format!("encrypt credentials: {e}")))?
        .to_json_value()
        .map_err(|e| ApiError::validation(e.to_string()))
}

pub(super) fn materialize_snapshot_row(
    snap: &DraftSnapshot,
    existing: Option<&repo::DraftRow>,
    is_super_admin: bool,
) -> Result<repo::DraftRow, ApiError> {
    if let Some(ref raw_cfg) = snap.config {
        reject_client_enc_in_supplier_config(raw_cfg)?;
    }
    if let Some(ref raw_eps) = snap.endpoints {
        reject_client_enc_in_endpoints(raw_eps)?;
    }
    if let Some(ref env) = snap.workbench_env {
        reject_client_enc_in_workbench_env(env)?;
    }
    if let Some(old_row) = existing {
        guard_draft_origin_pinning(
            Some(&snap.upstream_url),
            snap.config.as_ref(),
            snap.endpoints.as_ref(),
            &old_row.upstream_url,
            &old_row.config,
            &old_row.endpoints,
        )?;
    }
    let upstream_type = snap.upstream_type.clone();
    let mut config = snap
        .config
        .clone()
        .or_else(|| existing.map(|r| r.config.clone()))
        .unwrap_or_else(|| serde_json::json!({}));
    strip_workbench_env_from_config(&mut config);
    guard_supplier_tls_privilege(&config, existing.map(|r| &r.config), is_super_admin)?;
    if let Some(obj) = config.as_object_mut() {
        obj.entry("upstream_type")
            .or_insert_with(|| serde_json::Value::String(upstream_type.clone()));
    }
    let config_value = config_json_for_draft(&config, existing.map(|r| &r.config))?;

    let endpoints = match snap.endpoints.as_ref() {
        Some(raw_eps) => sanitize_and_encrypt_endpoints_tls(
            raw_eps,
            existing.map(|r| &r.endpoints),
            is_super_admin,
        )?,
        None => existing.map_or_else(|| serde_json::json!([]), |r| r.endpoints.clone()),
    };

    let db_workbench_env = match snap.workbench_env.as_ref() {
        Some(env) => {
            validate_workbench_env(env)?;
            encode_workbench_env_for_db(env)
        }
        None => existing.map_or_else(|| serde_json::json!([]), |r| r.workbench_env.clone()),
    };

    let now = chrono::Utc::now();
    Ok(repo::DraftRow {
        id: existing.map_or_else(Uuid::new_v4, |r| r.id),
        draft_code: existing
            .map_or_else(|| "ephemeral".to_string(), |r| r.draft_code.clone()),
        name: snap
            .name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| existing.map(|r| r.name.clone()))
            .unwrap_or_else(|| "ephemeral".to_string()),
        description: existing.and_then(|r| r.description.clone()),
        upstream_type,
        upstream_url: snap.upstream_url.clone(),
        config: config_value,
        endpoints,
        workbench_env: db_workbench_env,
        status: existing.map_or_else(|| "draft".to_string(), |r| r.status.clone()),
        source: existing.map_or_else(|| "manual".to_string(), |r| r.source.clone()),
        linked_supplier_id: existing.and_then(|r| r.linked_supplier_id),
        created_by: existing.and_then(|r| r.created_by.clone()),
        created_at: existing.map_or(now, |r| r.created_at),
        updated_at: now,
    })
}

pub(super) async fn evaluate_gate_items_for_row(
    pool: &PgPool,
    draft_id: Option<Uuid>,
    row: &repo::DraftRow,
    extra_runs: &[RecentGateRun],
) -> Result<Vec<DraftGateItem>, ApiError> {
    let env = parse_workbench_env(&row.workbench_env);
    let cfg = SupplierConfig::parse_and_validate(&row.config).ok();
    let mut recent: Vec<RecentGateRun> = extra_runs.to_vec();
    if let Some(did) = draft_id {
        let db_runs = repo::list_recent_gate_runs(pool, did, GATE_TTL_HOURS)
            .await
            .map_err(|e| ApiError::internal(format!("gate runs: {e}")))?;
        recent.extend(db_runs.into_iter().map(|r| RecentGateRun {
            endpoint_key: r.endpoint_key,
            spec_fingerprint: r.spec_fingerprint,
        }));
    }
    let eval = evaluate_endpoint_gates(
        &row.endpoints,
        &env,
        cfg.as_ref(),
        &row.upstream_url,
        &recent,
    );
    Ok(eval.items)
}

pub(super) async fn with_gate(pool: &PgPool, row: repo::DraftRow) -> Result<DraftDto, ApiError> {
    let items = evaluate_gate_items_for_row(pool, Some(row.id), &row, &[]).await?;
    let mut dto = draft_to_dto(row);
    dto.gate = items;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialize_snapshot_row_builds_ephemeral_and_preserves_existing_id() {
        let snap = DraftSnapshot {
            name: Some("未保存调试".to_string()),
            upstream_type: "rest".to_string(),
            upstream_url: "https://api.example.com/v1/ping".to_string(),
            config: Some(serde_json::json!({ "timeouts": { "total_ms": 5000 } })),
            endpoints: Some(serde_json::json!([{
                "key": "ep_ping",
                "name": "ping",
                "upstream_url": "https://api.example.com/v1/ping",
                "http_method": "GET"
            }])),
            workbench_env: Some(serde_json::json!({
                "version": 2,
                "active_env_id": null,
                "global": [{ "key": "token", "value": "abc", "enabled": true }],
                "environments": []
            })),
        };

        let fresh = materialize_snapshot_row(&snap, None, true).expect("ephemeral snapshot");
        assert_eq!(fresh.draft_code, "ephemeral");
        assert_eq!(fresh.name, "未保存调试");
        assert_eq!(fresh.upstream_url, "https://api.example.com/v1/ping");

        let mut existing = fresh.clone();
        let fixed_id = Uuid::new_v4();
        existing.id = fixed_id;
        existing.draft_code = "DR-TEST-01".to_string();

        let mut next_snap = snap;
        next_snap.upstream_url = "https://api.example.com/v2/ping".to_string();
        let merged =
            materialize_snapshot_row(&next_snap, Some(&existing), true).expect("merged snapshot");
        assert_eq!(merged.id, fixed_id);
        assert_eq!(merged.draft_code, "DR-TEST-01");
        assert_eq!(merged.upstream_url, "https://api.example.com/v2/ping");
        // Original row remains untouched in memory.
        assert_eq!(existing.upstream_url, "https://api.example.com/v1/ping");
    }
}
