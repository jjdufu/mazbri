use super::{
    ApiError, AuditAction, AuditLogRequest, AuditResourceType, AuditService, LiveRunRequest, PgPool,
    RollbackRequest, Row, RunDto, SupplierConfig, Uuid, connector_metrics, info, load_artifact_bytes,
    mask_config_credentials, repo,
};
use crate::services::bypass_probe::{
    self, BypassProbeClient, PROBE_PATH_BYPASS_GRPC, ProbeInvokeParams, ProbeOutcome,
};
use mazbri_core::hash_egress_spec_json;
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
        user_id: Some(user_id),
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

/// `live_run` 的入参（C1：经 bypass gRPC，不经 gateway 计费）。
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

    let invoke_req = bypass_probe::build_probe_invoke_request(&ProbeInvokeParams {
        channel_id: &prepared.channel_id,
        request_id: &prepared.request_id,
        timeout_ms: prepared.timeout_ms,
        payload_json: &prepared.payload_json,
        egress_spec_json: &prepared.egress_spec_json,
        supplier_id: &prepared.supplier_id,
        supplier_code: &prepared.supplier_code,
        supplier_config_json: &prepared.supplier_config_json,
    });

    let outcome = match bypass.execute_invoke(invoke_req).await {
        Ok(resp) => bypass_probe::outcome_from_invoke_response(&resp),
        Err(e) => ProbeOutcome {
            ok: false,
            latency_ms: 0,
            error_code: Some(if e.contains("BYPASS_CONNECT") {
                "BYPASS_UNAVAILABLE".into()
            } else {
                "BYPASS_GRPC_ERROR".into()
            }),
            error_message: Some(e),
            payload: None,
            status: None,
            url: None,
            adapter: None,
            biz_error: None,
            stages: serde_json::json!([{
                "name": "bypass_grpc",
                "ok": false,
                "response_summary": {"probe_path": PROBE_PATH_BYPASS_GRPC},
            }]),
        },
    };

    connector_metrics::record_run("live", outcome.ok, outcome.error_code.as_deref());

    let run_row = repo::insert_run(
        pool,
        repo::NewRun {
            draft_id: None,
            supplier_id: Some(supplier_id),
            mode: "live",
            ok: outcome.ok,
            latency_ms: Some(i32::saturating_from(outcome.latency_ms)),
            stages: &outcome.stages,
            assertions: &serde_json::json!([]),
            error_code: outcome.error_code.as_deref(),
            error_message: outcome.error_message.as_deref(),
            endpoint_key: None,
            testcase_id: None,
            actor: Some(actor),
            spec_fingerprint: None,
            probe_path: Some(PROBE_PATH_BYPASS_GRPC),
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
            "probe_path": PROBE_PATH_BYPASS_GRPC,
        })),
        ip_address: client_ip,
        ..Default::default()
    });

    let response_preview = outcome.payload.as_ref().map(|p| {
        serde_json::json!({
            "status": outcome.status.unwrap_or(if outcome.ok { 200 } else { 400 }),
            "body_preview": p.to_string().chars().take(4096).collect::<String>(),
            "url": outcome.url.clone().unwrap_or_default(),
            "adapter": outcome.adapter,
            "probe_path": PROBE_PATH_BYPASS_GRPC,
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
