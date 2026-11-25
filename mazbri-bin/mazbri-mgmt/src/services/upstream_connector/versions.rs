use super::{
    ApiError, AuditAction, AuditLogRequest, AuditResourceType, AuditService, LiveRunRequest, PgPool,
    RollbackRequest, Row, RunDto, SupplierConfig, Uuid, connector_metrics, info, load_artifact_bytes,
    mask_config_credentials, mask_endpoints_secrets, repo,
};
use crate::services::bypass_probe::{
    self, BypassProbeClient, PROBE_PATH_BYPASS_MGMT_PROBE, ProbeInvokeParams, ProbeOutcome,
};
use mazbri_core::hash_egress_spec_json;
use mazbri_core::num::SaturatingFrom;
use mazbri_upstream_runtime::mask_headers;

// ---------- Versions / Rollback / Live ----------

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_versions(
    pool: &PgPool,
    supplier_id: Uuid,
) -> Result<Vec<serde_json::Value>, ApiError> {
    let rows = repo::list_versions(pool, supplier_id)
        .await
        .map_err(|e| ApiError::internal(format!("list versions: {e}")))?;
    Ok(rows
        .into_iter()
        .map(|v| {
            serde_json::json!({
                "id": v.id,
                "supplier_id": v.supplier_id,
                "version_no": v.version_no,
                "config_snapshot": mask_config_credentials(v.config_snapshot),
                "channels_snapshot": v.channels_snapshot,
                "endpoints_snapshot": mask_endpoints_secrets(v.endpoints_snapshot),
                "comment": v.comment,
                "published_by": v.published_by,
                "created_at": v.created_at,
            })
        })
        .collect())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn version_diff(
    pool: &PgPool,
    supplier_id: Uuid,
    version_id: Uuid,
) -> Result<serde_json::Value, ApiError> {
    let ver = repo::get_version(pool, version_id)
        .await
        .map_err(|e| ApiError::internal(format!("get version: {e}")))?
        .ok_or_else(|| ApiError::not_found("version".into(), version_id.to_string()))?;
    if ver.supplier_id != supplier_id {
        return Err(ApiError::not_found(
            "version".into(),
            version_id.to_string(),
        ));
    }
    let current: Option<(serde_json::Value,)> =
        sqlx::query_as::<_, (serde_json::Value,)>("SELECT config FROM suppliers WHERE id = $1")
            .bind(supplier_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| ApiError::internal(format!("get supplier: {e}")))?;
    let current_cfg = current
        .map(|c| mask_config_credentials(c.0))
        .unwrap_or(serde_json::json!({}));

    Ok(serde_json::json!({
        "version_id": ver.id,
        "version_no": ver.version_no,
        "version_config": mask_config_credentials(ver.config_snapshot),
        "current_config": current_cfg,
        "channels_snapshot": ver.channels_snapshot,
        "endpoints_snapshot": mask_endpoints_secrets(ver.endpoints_snapshot),
    }))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub struct RollbackParams<'a> {
    pub audit: &'a AuditService,
    pub ssrf: &'a super::SsrfChecker,
    pub supplier_id: Uuid,
    pub req: RollbackRequest,
    pub actor: &'a str,
    pub user_id: Uuid,
    pub is_super_admin: bool,
    pub client_ip: Option<String>,
}

async fn validate_rolled_back_supplier(
    ssrf: &super::SsrfChecker,
    rolled_cfg: &SupplierConfig,
    raw_snapshot: &serde_json::Value,
    old_cfg: &serde_json::Value,
    upstream_url: &str,
    is_super_admin: bool,
) -> Result<(), ApiError> {
    super::guard_supplier_tls_privilege(raw_snapshot, Some(old_cfg), is_super_admin)?;
    rolled_cfg
        .validate_upstream_url_for_type(upstream_url)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;

    if rolled_cfg.is_transparent() {
        super::ssrf::validate_public_http_url(ssrf, upstream_url)
            .await
            .map_err(|e| ApiError::validation(format!("SSRF_BLOCKED: {e}")))?;
        if let Some(ref auth) = rolled_cfg.auth_flow
            && auth.enabled
            && !auth.token_url.trim().is_empty()
        {
            super::ssrf::validate_public_http_url(ssrf, &auth.token_url)
                .await
                .map_err(|e| ApiError::validation(format!("SSRF_BLOCKED token_url: {e}")))?;
        }
    }
    Ok(())
}

async fn insert_rollback_version_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    supplier_id: Uuid,
    ver: &repo::VersionRow,
    reason: Option<&str>,
    actor: &str,
) -> Result<(i32, Uuid), ApiError> {
    let version_no: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_no), 0) + 1 FROM upstream_connector_versions WHERE supplier_id = $1",
    )
    .bind(supplier_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| ApiError::internal(format!("version no: {e}")))?;

    let comment = format!(
        "rollback to v{}: {}",
        ver.version_no,
        reason.unwrap_or_default()
    );
    let new_version_id: Uuid = sqlx::query_scalar(
        r"
        INSERT INTO upstream_connector_versions
            (supplier_id, version_no, config_snapshot, channels_snapshot,
             endpoints_snapshot, comment, published_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        ",
    )
    .bind(supplier_id)
    .bind(version_no)
    .bind(&ver.config_snapshot)
    .bind(&ver.channels_snapshot)
    .bind(&ver.endpoints_snapshot)
    .bind(&comment)
    .bind(actor)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| ApiError::internal(format!("insert rollback version: {e}")))?;
    Ok((version_no, new_version_id))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn rollback(
    pool: &PgPool,
    params: RollbackParams<'_>,
) -> Result<serde_json::Value, ApiError> {
    let RollbackParams {
        audit,
        ssrf,
        supplier_id,
        req,
        actor,
        user_id,
        is_super_admin,
        client_ip,
    } = params;

    let ver = repo::get_version(pool, req.version_id)
        .await
        .map_err(|e| ApiError::internal(format!("get version: {e}")))?
        .ok_or_else(|| ApiError::not_found("version".into(), req.version_id.to_string()))?;
    if ver.supplier_id != supplier_id {
        return Err(ApiError::not_found(
            "version".into(),
            req.version_id.to_string(),
        ));
    }

    super::guard_endpoints_tls_privilege(&ver.endpoints_snapshot, is_super_admin)?;
    let rolled_cfg = SupplierConfig::parse_and_validate(&ver.config_snapshot)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?
        .normalize_disabled_sections();

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::internal(format!("begin rollback tx: {e}")))?;

    let supplier_row: Option<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT upstream_url, config FROM suppliers WHERE id = $1 FOR UPDATE",
    )
    .bind(supplier_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| ApiError::internal(format!("lock supplier: {e}")))?;

    let Some((upstream_url, old_cfg)) = supplier_row else {
        return Err(ApiError::not_found(
            "supplier".into(),
            supplier_id.to_string(),
        ));
    };

    validate_rolled_back_supplier(
        ssrf,
        &rolled_cfg,
        &ver.config_snapshot,
        &old_cfg,
        &upstream_url,
        is_super_admin,
    )
    .await?;

    sqlx::query("UPDATE suppliers SET config = $2, updated_at = NOW() WHERE id = $1")
        .bind(supplier_id)
        .bind(&ver.config_snapshot)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal(format!("rollback update: {e}")))?;

    let (version_no, new_version_id) =
        insert_rollback_version_tx(&mut tx, supplier_id, &ver, req.reason.as_deref(), actor)
            .await?;

    tx.commit()
        .await
        .map_err(|e| ApiError::internal(format!("commit rollback tx: {e}")))?;

    audit.log(AuditLogRequest {
        user_id: Some(user_id),
        action: AuditAction::ConnectorRollback,
        resource_type: AuditResourceType::Supplier,
        resource_id: Some(supplier_id.to_string()),
        details: Some(serde_json::json!({
            "from_version_id": req.version_id,
            "new_version_id": new_version_id,
            "reason": req.reason,
        })),
        ip_address: client_ip,
        old_value: Some(mask_config_credentials(old_cfg)),
        new_value: Some(mask_config_credentials(ver.config_snapshot)),
        ..Default::default()
    });

    Ok(serde_json::json!({
        "ok": true,
        "version_id": new_version_id,
        "version_no": version_no,
    }))
}

/// `live_run` 的入参（经 bypass 管理探针 HTTP，不经 gateway 计费）。
pub struct LiveRunParams<'a> {
    pub audit: &'a AuditService,
    pub bypass: &'a BypassProbeClient,
    pub supplier_id: Uuid,
    pub req: LiveRunRequest,
    pub actor: &'a str,
    pub user_id: Uuid,
    pub client_ip: Option<String>,
    pub default_timeout_ms: u64,
    pub max_body_bytes: usize,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn live_run(pool: &PgPool, params: LiveRunParams<'_>) -> Result<RunDto, ApiError> {
    let LiveRunParams {
        audit,
        bypass,
        supplier_id,
        req,
        actor,
        user_id,
        client_ip,
        default_timeout_ms,
        max_body_bytes,
    } = params;
    let prepared =
        prepare_live_bypass(pool, supplier_id, req, default_timeout_ms, max_body_bytes).await?;

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

    connector_metrics::record_run("live", outcome.ok, outcome.error_code.as_deref());

    let run_row = repo::insert_run(
        pool,
        repo::NewRun {
            draft_id: None,
            supplier_id: Some(supplier_id),
            mode: "live",
            ok: outcome.ok,
            latency_ms: Some(i32::saturating_from(outcome.upstream_wire_ms)),
            stages: &outcome.stages,
            assertions: &serde_json::json!([]),
            error_code: outcome.error_code.as_deref(),
            error_message: outcome.error_message.as_deref(),
            endpoint_key: None,
            testcase_id: None,
            actor: Some(actor),
            spec_fingerprint: None,
            probe_path: Some(PROBE_PATH_BYPASS_MGMT_PROBE),
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("insert run: {e}")))?;

    audit.log(AuditLogRequest {
        user_id: Some(user_id),
        action: AuditAction::ConnectorRun,
        resource_type: AuditResourceType::Supplier,
        resource_id: Some(supplier_id.to_string()),
        details: Some(serde_json::json!({
            "mode": "live",
            "ok": outcome.ok,
            "probe_path": PROBE_PATH_BYPASS_MGMT_PROBE,
        })),
        ip_address: client_ip,
        ..Default::default()
    });

    let response_preview = outcome.payload.as_ref().map(|p| {
        serde_json::json!({
            "status": outcome.status.unwrap_or(if outcome.ok { 200 } else { 400 }),
            "headers": mask_headers(&outcome.headers, true),
            "body_preview": p.to_string().chars().take(4096).collect::<String>(),
            "url": outcome.url.clone().unwrap_or_default(),
            "adapter": outcome.adapter,
            "request_id": prepared.request_id,
            "upstream_request_id": outcome.upstream_request_id,
            "trace_id": outcome.trace_id,
            "probe_path": PROBE_PATH_BYPASS_MGMT_PROBE,
        })
    });
    Ok(super::run::run_dto_from_row(run_row, response_preview))
}

/// Delete probe runs older than `retention_days`. Returns deleted row count.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn cleanup_old_runs(pool: &PgPool, retention_days: u32) -> Result<u64, ApiError> {
    if retention_days == 0 {
        return Ok(0);
    }
    let n = repo::delete_runs_older_than_days(pool, i32::saturating_from(retention_days))
        .await
        .map_err(|e| ApiError::internal(format!("cleanup runs: {e}")))?;
    if n > 0 {
        info!(deleted = n, retention_days, "connector runs cleaned up");
    }
    Ok(n)
}

struct PreparedLiveBypass {
    channel_id: String,
    request_id: String,
    timeout_ms: u64,
    payload_json: String,
    egress_spec_json: String,
    supplier_id: String,
    supplier_code: String,
    supplier_config_json: String,
}

/// 线上试跑：已发布通道 egress + 供应商密文 config → bypass Invoke（同生产进程路径，不经 gateway）。
async fn prepare_live_bypass(
    pool: &PgPool,
    supplier_id: Uuid,
    req: LiveRunRequest,
    default_timeout_ms: u64,
    max_body_bytes: usize,
) -> Result<PreparedLiveBypass, ApiError> {
    let row = sqlx::query("SELECT id, supplier_code, config FROM suppliers WHERE id = $1")
        .bind(supplier_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| ApiError::internal(format!("get supplier: {e}")))?
        .ok_or_else(|| ApiError::not_found("supplier".into(), supplier_id.to_string()))?;
    let supplier_code: String = row.get("supplier_code");
    let config: serde_json::Value = row.get("config");

    let channel_code = req.channel_code.trim();
    let channel =
        sqlx::query("SELECT id, config FROM channels WHERE supplier_id = $1 AND channel_code = $2")
            .bind(supplier_id)
            .bind(channel_code)
            .fetch_optional(pool)
            .await
            .map_err(|e| ApiError::internal(format!("get channel: {e}")))?
            .ok_or_else(|| ApiError::not_found("channel".into(), channel_code.to_string()))?;
    let channel_id: Uuid = channel.get("id");
    let channel_config: serde_json::Value = channel.get("config");
    let spec = mazbri_core::parse_channel_config_egress(&channel_config)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;

    // 校验可解析；密钥保持密文交给 bypass 解密（mgmt 不 decrypt）。
    let cfg = SupplierConfig::parse_and_validate(&config)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    let supplier_config_json = serde_json::to_string(&cfg)
        .map_err(|e| ApiError::internal(format!("serialize supplier config: {e}")))?;

    let payload_json = req.payload.to_string();
    if payload_json.len() > max_body_bytes {
        return Err(ApiError::validation(format!(
            "request body exceeds max_body_bytes ({max_body_bytes})"
        )));
    }

    let timeout_ms = spec
        .timeout_ms
        .or_else(|| cfg.timeouts.as_ref().and_then(|t| t.total_ms))
        .unwrap_or(default_timeout_ms);

    if let Some(aid) = spec.artifact_id.as_deref() {
        let _ = load_artifact_bytes(pool, aid).await?;
    }

    let egress_spec_json = serde_json::to_string(&spec)
        .map_err(|e| ApiError::internal(format!("serialize egress: {e}")))?;
    let _ = hash_egress_spec_json(&egress_spec_json);

    Ok(PreparedLiveBypass {
        channel_id: channel_id.to_string(),
        request_id: Uuid::new_v4().to_string(),
        timeout_ms,
        payload_json,
        egress_spec_json,
        supplier_id: supplier_id.to_string(),
        supplier_code,
        supplier_config_json,
    })
}
