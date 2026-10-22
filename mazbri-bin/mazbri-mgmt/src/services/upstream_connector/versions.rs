use super::{
    ApiError, Arc, AuditAction, AuditLogRequest, AuditResourceType, AuditService, Duration,
    ExecuteMode, ExecuteOptions, InvokeInput, LiveRunRequest, PgPool, RollbackRequest, Row, RunDto,
    SupplierConfig, Uuid, connector_metrics, info, load_artifact_bytes, mask_config_credentials,
    repo, runtime_engine,
};
use mazbri_core::num::SaturatingFrom;

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
                "endpoints_snapshot": v.endpoints_snapshot,
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
        "endpoints_snapshot": ver.endpoints_snapshot,
    }))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn rollback(
    pool: &PgPool,
    audit: &AuditService,
    supplier_id: Uuid,
    req: RollbackRequest,
    actor: &str,
    user_id: Uuid,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
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

    let old: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT config FROM suppliers WHERE id = $1")
            .bind(supplier_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| ApiError::internal(format!("get supplier: {e}")))?;

    sqlx::query("UPDATE suppliers SET config = $2, updated_at = NOW() WHERE id = $1")
        .bind(supplier_id)
        .bind(&ver.config_snapshot)
        .execute(pool)
        .await
        .map_err(|e| ApiError::internal(format!("rollback update: {e}")))?;

    // New version snapshot for rollback action
    let version_no = repo::next_version_no(pool, supplier_id)
        .await
        .map_err(|e| ApiError::internal(format!("version no: {e}")))?;
    let comment = format!(
        "rollback to v{}: {}",
        ver.version_no,
        req.reason.clone().unwrap_or_default()
    );
    let new_ver = repo::insert_version(
        pool,
        repo::NewVersion {
            supplier_id,
            version_no,
            config_snapshot: &ver.config_snapshot,
            channels_snapshot: &ver.channels_snapshot,
            endpoints_snapshot: &ver.endpoints_snapshot,
            comment: Some(&comment),
            published_by: Some(actor),
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("insert rollback version: {e}")))?;

    audit.log(AuditLogRequest {
        user_id,
        action: AuditAction::ConnectorRollback,
        resource_type: AuditResourceType::Supplier,
        resource_id: Some(supplier_id.to_string()),
        details: Some(serde_json::json!({
            "from_version_id": req.version_id,
            "new_version_id": new_ver.id,
            "reason": req.reason,
        })),
        ip_address: client_ip,
        old_value: old.map(mask_config_credentials),
        new_value: Some(mask_config_credentials(ver.config_snapshot)),
        ..Default::default()
    });

    Ok(serde_json::json!({
        "ok": true,
        "version_id": new_ver.id,
        "version_no": new_ver.version_no,
    }))
}

/// `live_run` 的入参。
pub struct LiveRunParams<'a> {
    pub audit: &'a AuditService,
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
        supplier_id,
        req,
        actor,
        user_id,
        client_ip,
        default_timeout_ms,
        max_body_bytes,
    } = params;
    let input =
        prepare_live_input(pool, supplier_id, req, default_timeout_ms, max_body_bytes).await?;
    let result = runtime_engine().invoke(input).await;
    let stages_json = serde_json::to_value(&result.stages).unwrap_or(serde_json::json!([]));

    connector_metrics::record_run("live", result.ok, result.error_code.as_deref());

    let run_row = repo::insert_run(
        pool,
        repo::NewRun {
            draft_id: None,
            supplier_id: Some(supplier_id),
            mode: "live",
            ok: result.ok,
            latency_ms: Some(i32::saturating_from(result.latency_ms)),
            stages: &stages_json,
            assertions: &serde_json::json!([]),
            error_code: result.error_code.as_deref(),
            error_message: result.error_message.as_deref(),
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
        resource_id: Some(supplier_id.to_string()),
        details: Some(serde_json::json!({"mode":"live","ok": result.ok})),
        ip_address: client_ip,
        ..Default::default()
    });

    let response_preview = result.response.map(|r| {
        serde_json::json!({
            "status": r.status,
            "body_preview": String::from_utf8_lossy(&r.body).chars().take(4096).collect::<String>(),
            "url": r.final_url,
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

/// 线上试跑入参：已发布通道 `channels.config.egress` + 供应商配置，`mode=Live`。
async fn prepare_live_input(
    pool: &PgPool,
    supplier_id: Uuid,
    req: LiveRunRequest,
    default_timeout_ms: u64,
    max_body_bytes: usize,
) -> Result<InvokeInput, ApiError> {
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

    let cfg = SupplierConfig::parse_and_validate(&config)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?
        .decrypt_credentials_for_runtime()
        .map_err(|e| ApiError::internal(format!("decrypt: {e}")))?;

    let payload = req.payload;
    if payload.to_string().len() > max_body_bytes {
        return Err(ApiError::validation(format!(
            "request body exceeds max_body_bytes ({max_body_bytes})"
        )));
    }

    let timeout_ms = spec
        .timeout_ms
        .or_else(|| cfg.timeouts.as_ref().and_then(|t| t.total_ms))
        .unwrap_or(default_timeout_ms);
    let credentials = cfg
        .auth_flow
        .as_ref()
        .map(|auth| auth.credentials.clone())
        .unwrap_or_default();
    let artifact_bytes = match spec.artifact_id.as_deref() {
        Some(aid) => load_artifact_bytes(pool, aid).await?,
        None => None,
    };

    Ok(InvokeInput {
        supplier_id: supplier_id.to_string(),
        supplier_code,
        channel_id: channel_id.to_string(),
        request_id: Uuid::new_v4().to_string(),
        payload,
        spec,
        config: Some(Arc::new(cfg)),
        timeout: Duration::from_millis(timeout_ms),
        mode: ExecuteMode::Live,
        options: ExecuteOptions {
            follow_redirects: false,
            max_body_capture_bytes: 64 * 1024,
            mask_secrets: true,
            collect_trace: true,
        },
        credentials,
        artifact_bytes,
        cache_key: Some(format!("live:{supplier_id}")),
    })
}
