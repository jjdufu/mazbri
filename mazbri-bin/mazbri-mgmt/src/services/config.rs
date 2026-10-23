use crate::repositories::config as repo;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;

/// Default plan QPS (customer usage plan). JSON field: `max_concurrent`.
#[must_use]
pub fn default_max_concurrent() -> i32 {
    1000
}
/// 0 = do not enforce tenant bandwidth (local gateway skips bandwidth bucket).
#[must_use]
pub fn default_max_bandwidth_mbps() -> i32 {
    0
}
/// Default plan burst capacity.
#[must_use]
pub fn default_burst_size() -> i32 {
    100
}
#[must_use]
pub fn default_enabled() -> bool {
    true
}
#[must_use]
pub fn default_cache_ttl() -> i32 {
    300
}
#[must_use]
pub fn default_max_cache_size() -> i32 {
    1_073_741_824
}
#[must_use]
pub fn default_tenant_isolated() -> bool {
    true
}

/// Customer usage-plan rate-limit update (tenant policy).
/// Enforced by gateway local token bucket only — not a third product/channel engine.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitConfigUpdateRequest {
    /// Plan rate QPS (套餐速率), JSON field `max_concurrent`.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: i32,
    /// Bandwidth cap in Mbps; 0 = unlimited.
    #[serde(default = "default_max_bandwidth_mbps")]
    pub max_bandwidth_mbps: i32,
    /// Optional per-API overrides (advanced). Each value must be ≤ plan QPS. Empty = no per-API tighten.
    #[serde(default)]
    pub api_limits: HashMap<String, i32>,
    /// Plan burst capacity.
    #[serde(default = "default_burst_size")]
    pub burst_size: i32,
    /// Per-client IP QPS; when omitted, follows plan QPS (`max_concurrent`).
    #[serde(default)]
    pub client_ip_qps: Option<i32>,
    /// Per-client IP burst; when omitted, follows `burst_size`.
    #[serde(default)]
    pub client_ip_burst: Option<i32>,
    /// Enable customer plan rate limits.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub expected_updated_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl Default for RateLimitConfigUpdateRequest {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            max_bandwidth_mbps: default_max_bandwidth_mbps(),
            api_limits: HashMap::new(),
            burst_size: default_burst_size(),
            client_ip_qps: None,
            client_ip_burst: None,
            enabled: default_enabled(),
            expected_updated_at: None,
        }
    }
}

/// Resolve IP-dimension limits: omitted fields follow tenant knobs.
#[must_use]
pub fn resolve_client_ip_limits(config: &RateLimitConfigUpdateRequest) -> (i32, i32) {
    let client_ip_qps = config.client_ip_qps.unwrap_or(config.max_concurrent);
    let client_ip_burst = config.client_ip_burst.unwrap_or(config.burst_size);
    (client_ip_qps, client_ip_burst)
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateCacheConfigRequest {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_cache_ttl")]
    pub default_ttl: i32,
    #[serde(default = "default_max_cache_size")]
    pub max_cache_size: i32,
    #[serde(default)]
    pub api_ttl: HashMap<String, i32>,
    #[serde(default = "default_tenant_isolated")]
    pub tenant_isolated: bool,
    #[serde(default)]
    pub warmup_enabled: bool,
    #[serde(default)]
    pub warmup_keys: Vec<String>,
    #[serde(default)]
    pub expected_updated_at: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Debug)]
pub enum ConfigUpdateError {
    TenantNotFound(String),
    Validation(String),
    Conflict(String),
    Internal(String),
}
fn config_conflict_message(scope: &str, updated_at: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "{scope} was modified by another operator at {}. Reload and retry.",
        updated_at.to_rfc3339()
    )
}
/// Validate customer usage-plan rate-limit fields (plan QPS + abuse + optional API overrides).
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_ratelimit_config(config: &RateLimitConfigUpdateRequest) -> Result<(), String> {
    if config.max_concurrent <= 0 {
        return Err("plan QPS (max_concurrent) must be greater than 0".to_string());
    }
    if config.max_bandwidth_mbps < 0 {
        return Err("max_bandwidth_mbps must be non-negative".to_string());
    }
    if config.burst_size <= 0 {
        return Err("burst_size must be greater than 0".to_string());
    }
    let (client_ip_qps, client_ip_burst) = resolve_client_ip_limits(config);
    if client_ip_qps <= 0 {
        return Err("client_ip_qps must be greater than 0".to_string());
    }
    if client_ip_burst <= 0 {
        return Err("client_ip_burst must be greater than 0".to_string());
    }
    if client_ip_qps > config.max_concurrent {
        return Err("client_ip_qps must not exceed plan QPS (max_concurrent)".to_string());
    }
    if client_ip_burst > config.burst_size {
        return Err("client_ip_burst must not exceed burst_size".to_string());
    }
    // Per-API overrides must not exceed plan QPS (customer usage plan fairness).
    for (path, limit) in &config.api_limits {
        if *limit <= 0 {
            return Err(format!("api_limits entry '{path}' must be greater than 0"));
        }
        if *limit > config.max_concurrent {
            return Err(format!(
                "api_limits entry '{path}' ({limit}) must not exceed plan QPS (max_concurrent = {})",
                config.max_concurrent
            ));
        }
    }
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_cache_config(config: &UpdateCacheConfigRequest) -> Result<(), String> {
    if config.default_ttl < 1 {
        return Err("default_ttl must be at least 1 second".to_string());
    }
    if config.default_ttl > 86400 {
        return Err("default_ttl must not exceed 86400 seconds (24 hours)".to_string());
    }
    if config.max_cache_size < 1024 {
        return Err("max_cache_size must be at least 1024 bytes".to_string());
    }
    if config.warmup_keys.len() > 1000 {
        return Err("warmup_keys must not exceed 1000 entries".to_string());
    }
    Ok(())
}
/// Product contract: enforcement is local per gateway instance (not cluster-strict).
pub const RATE_LIMIT_ENFORCEMENT_MODEL: &str = "local_per_instance";
/// Human-readable multi-replica semantics for UI/API consumers.
pub const RATE_LIMIT_CLUSTER_NOTE: &str = "Limits apply per gateway instance. Aggregate capacity scales approximately with replica count.";

/// GET response for customer usage-plan rate limits.
/// Product names: plan QPS (`max_concurrent`), burst, abuse knobs, optional API overrides.
#[derive(Debug, Serialize)]
pub struct RateLimitConfigResponse {
    pub tenant_code: String,
    pub tenant_id: String,
    /// Plan rate QPS (套餐速率).
    pub max_concurrent: i32,
    /// Bandwidth cap Mbps; 0 = unlimited.
    pub max_bandwidth_mbps: i32,
    /// Optional per-API overrides; empty means no per-API dimension on gateway.
    pub api_limits: serde_json::Value,
    /// Plan burst capacity.
    pub burst_size: i32,
    pub client_ip_qps: i32,
    pub client_ip_burst: i32,
    /// Whether plan rate limits are enabled.
    pub enabled: bool,
    /// Read-only product contract: always `local_per_instance`.
    pub enforcement_model: String,
    /// Read-only multi-replica capacity note (must stay visible to operators).
    pub cluster_note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

fn rate_limit_contract_fields() -> (String, String) {
    (
        RATE_LIMIT_ENFORCEMENT_MODEL.to_string(),
        RATE_LIMIT_CLUSTER_NOTE.to_string(),
    )
}

#[must_use]
pub fn build_default_ratelimit_response(
    tenant_code: String,
    tenant_id_str: String,
) -> RateLimitConfigResponse {
    let tenant_qps = default_max_concurrent();
    let burst = default_burst_size();
    let (enforcement_model, cluster_note) = rate_limit_contract_fields();
    RateLimitConfigResponse {
        tenant_code,
        tenant_id: tenant_id_str,
        max_concurrent: tenant_qps,
        max_bandwidth_mbps: default_max_bandwidth_mbps(),
        api_limits: serde_json::json!({}),
        burst_size: burst,
        client_ip_qps: tenant_qps,
        client_ip_burst: burst,
        enabled: default_enabled(),
        enforcement_model,
        cluster_note,
        created_at: None,
        updated_at: None,
        message: Some("Default configuration (no custom config set)".to_string()),
    }
}
#[must_use]
pub fn row_to_ratelimit_response(
    row: &repo::RateLimitConfigRow,
    tenant_code: String,
    tenant_id_str: String,
) -> RateLimitConfigResponse {
    let (enforcement_model, cluster_note) = rate_limit_contract_fields();
    RateLimitConfigResponse {
        tenant_code,
        tenant_id: tenant_id_str,
        max_concurrent: row.max_concurrent,
        max_bandwidth_mbps: row.max_bandwidth_mbps,
        api_limits: row.api_limits.clone(),
        burst_size: row.burst_size,
        client_ip_qps: row.client_ip_qps,
        client_ip_burst: row.client_ip_burst,
        enabled: row.enabled,
        enforcement_model,
        cluster_note,
        created_at: Some(row.created_at),
        updated_at: Some(row.updated_at),
        message: None,
    }
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_ratelimit_config(
    db: &PgPool,
    ratelimit_db: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<serde_json::Value, String> {
    let identities = crate::repositories::tenant::get_tenant_identities(db, &[tenant_id])
        .await
        .map_err(|e| format!("Failed to fetch tenant identity: {e}"))?;
    let (tenant_code, tenant_id_str) = identities
        .get(&tenant_id)
        .cloned()
        .ok_or_else(|| format!("Tenant not found: {tenant_id}"))?;
    if let Some(row) = repo::get_ratelimit_config(ratelimit_db, tenant_id)
        .await
        .map_err(|e| format!("Failed to get rate limit config: {e}"))?
    {
        let resp = row_to_ratelimit_response(&row, tenant_code, tenant_id_str);
        Ok(serde_json::to_value(resp).unwrap_or_default())
    } else {
        let resp = build_default_ratelimit_response(tenant_code, tenant_id_str);
        Ok(serde_json::to_value(resp).unwrap_or_default())
    }
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_ratelimit_config(
    db: &PgPool,
    ratelimit_db: &PgPool,
    tenant_id: uuid::Uuid,
    payload: &RateLimitConfigUpdateRequest,
) -> Result<serde_json::Value, ConfigUpdateError> {
    let identities = crate::repositories::tenant::get_tenant_identities(db, &[tenant_id])
        .await
        .map_err(|e| {
            ConfigUpdateError::Internal(format!("Failed to fetch tenant identity: {e}"))
        })?;
    let (tenant_code, tenant_id_str) = identities.get(&tenant_id).cloned().ok_or_else(|| {
        ConfigUpdateError::TenantNotFound(format!("Tenant not found: {tenant_id}"))
    })?;
    validate_ratelimit_config(payload).map_err(ConfigUpdateError::Validation)?;
    let (client_ip_qps, client_ip_burst) = resolve_client_ip_limits(payload);
    let api_limits_json =
        serde_json::to_value(&payload.api_limits).unwrap_or_else(|_| serde_json::json!({}));
    let write_params = repo::RateLimitWriteParams {
        max_concurrent: payload.max_concurrent,
        max_bandwidth_mbps: payload.max_bandwidth_mbps,
        api_limits: &api_limits_json,
        burst_size: payload.burst_size,
        client_ip_qps,
        client_ip_burst,
        enabled: payload.enabled,
    };
    let row = match payload.expected_updated_at {
        Some(expected_updated_at) => repo::update_ratelimit_config_if_unchanged(
            ratelimit_db,
            tenant_id,
            &write_params,
            expected_updated_at,
        )
        .await
        .map_err(|e| {
            ConfigUpdateError::Internal(format!("Failed to update rate limit config: {e}"))
        })?,
        None => repo::insert_ratelimit_config(ratelimit_db, tenant_id, &write_params)
            .await
            .map_err(|e| {
                ConfigUpdateError::Internal(format!("Failed to insert rate limit config: {e}"))
            })?,
    };
    let Some(row) = row else {
        let message = match repo::get_ratelimit_config(ratelimit_db, tenant_id)
            .await
            .map_err(|e| {
                ConfigUpdateError::Internal(format!(
                    "Failed to read current rate limit config: {e}"
                ))
            })? {
            Some(current) => config_conflict_message("Rate limit config", current.updated_at),
            None => "Rate limit config changed during save. Reload and retry.".to_string(),
        };
        return Err(ConfigUpdateError::Conflict(message));
    };
    let resp = row_to_ratelimit_response(&row, tenant_code, tenant_id_str);
    Ok(serde_json::to_value(resp).unwrap_or_default())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_global_ratelimit_settings(
    db: &PgPool,
    ratelimit_db: &PgPool,
) -> Result<serde_json::Value, String> {
    let configs = repo::list_enabled_ratelimit_configs(ratelimit_db)
        .await
        .map_err(|e| format!("Failed to get global rate limit settings: {e}"))?;
    let tenant_ids: Vec<uuid::Uuid> = configs.iter().map(|c| c.tenant_id).collect();
    let identities = crate::repositories::tenant::get_tenant_identities(db, &tenant_ids)
        .await
        .map_err(|e| format!("Failed to fetch tenant identities: {e}"))?;
    let responses: Vec<RateLimitConfigResponse> = configs
        .iter()
        .map(|row| {
            let (tenant_code, tenant_id_str) = identities
                .get(&row.tenant_id)
                .cloned()
                .unwrap_or_else(|| ("UNKNOWN".to_string(), "UNKNOWN".to_string()));
            row_to_ratelimit_response(row, tenant_code, tenant_id_str)
        })
        .collect();
    Ok(serde_json::json!({ "configs" : responses, "total" : responses.len(), }))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_ratelimit_config(
    ratelimit_db: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<bool, String> {
    let affected = repo::delete_ratelimit_config(ratelimit_db, tenant_id)
        .await
        .map_err(|e| format!("Failed to delete rate limit config: {e}"))?;
    Ok(affected > 0)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_cache_config(
    db: &PgPool,
    cache_db: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<serde_json::Value, String> {
    let identities = crate::repositories::tenant::get_tenant_identities(db, &[tenant_id])
        .await
        .map_err(|e| format!("Failed to fetch tenant identity: {e}"))?;
    let (tenant_code, tenant_id_str) = identities
        .get(&tenant_id)
        .cloned()
        .ok_or_else(|| format!("Tenant not found: {tenant_id}"))?;
    match repo::get_cache_config(cache_db, tenant_id)
        .await
        .map_err(|e| format!("Failed to get cache config: {e}"))?
    {
        Some(row) => Ok(serde_json::json!(
            { "tenant_code" : tenant_code, "tenant_id" : tenant_id_str, "enabled"
            : row.enabled, "default_ttl" : row.default_ttl, "max_cache_size" :
            row.max_cache_size, "api_ttl" : row.api_ttl, "tenant_isolated" : row
            .tenant_isolated, "warmup_enabled" : row.warmup_enabled,
            "warmup_keys" : row.warmup_keys, "created_at" : row.created_at,
            "updated_at" : row.updated_at, }
        )),
        None => Ok(serde_json::json!(
            { "tenant_code" : tenant_code, "tenant_id" : tenant_id_str, "enabled"
            : true, "default_ttl" : 300, "max_cache_size" : 1_073_741_824, "api_ttl"
            : {}, "tenant_isolated" : true, "warmup_enabled" : false,
            "warmup_keys" : [], "message" :
            "Default configuration (no custom config set)", }
        )),
    }
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_cache_config(
    db: &PgPool,
    cache_db: &PgPool,
    tenant_id: uuid::Uuid,
    payload: &UpdateCacheConfigRequest,
) -> Result<serde_json::Value, ConfigUpdateError> {
    let identities = crate::repositories::tenant::get_tenant_identities(db, &[tenant_id])
        .await
        .map_err(|e| {
            ConfigUpdateError::Internal(format!("Failed to fetch tenant identity: {e}"))
        })?;
    let (tenant_code, tenant_id_str) = identities.get(&tenant_id).cloned().ok_or_else(|| {
        ConfigUpdateError::TenantNotFound(format!("Tenant not found: {tenant_id}"))
    })?;
    validate_cache_config(payload).map_err(ConfigUpdateError::Validation)?;
    let api_ttl_json =
        serde_json::to_value(&payload.api_ttl).unwrap_or_else(|_| serde_json::json!({}));
    let write_params = repo::CacheWriteParams {
        enabled: payload.enabled,
        default_ttl: payload.default_ttl,
        max_cache_size: payload.max_cache_size,
        api_ttl: &api_ttl_json,
        tenant_isolated: payload.tenant_isolated,
        warmup_enabled: payload.warmup_enabled,
        warmup_keys: &payload.warmup_keys,
    };
    let row = match payload.expected_updated_at {
        Some(expected_updated_at) => repo::update_cache_config_if_unchanged(
            cache_db,
            tenant_id,
            &write_params,
            expected_updated_at,
        )
        .await
        .map_err(|e| ConfigUpdateError::Internal(format!("Failed to update cache config: {e}")))?,
        None => repo::insert_cache_config(cache_db, tenant_id, &write_params)
            .await
            .map_err(|e| {
                ConfigUpdateError::Internal(format!("Failed to insert cache config: {e}"))
            })?,
    };
    let Some(row) = row else {
        let message = match repo::get_cache_config(cache_db, tenant_id)
            .await
            .map_err(|e| {
                ConfigUpdateError::Internal(format!("Failed to read current cache config: {e}"))
            })? {
            Some(current) => config_conflict_message("Cache config", current.updated_at),
            None => "Cache config changed during save. Reload and retry.".to_string(),
        };
        return Err(ConfigUpdateError::Conflict(message));
    };
    Ok(serde_json::json!(
        { "tenant_code" : tenant_code, "tenant_id" : tenant_id_str, "enabled" : row
        .enabled, "default_ttl" : row.default_ttl, "max_cache_size" : row
        .max_cache_size, "api_ttl" : row.api_ttl, "tenant_isolated" : row
        .tenant_isolated, "warmup_enabled" : row.warmup_enabled, "warmup_keys" : row
        .warmup_keys, "created_at" : row.created_at, "updated_at" : row.updated_at, }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_plan() -> RateLimitConfigUpdateRequest {
        RateLimitConfigUpdateRequest {
            max_concurrent: 100,
            max_bandwidth_mbps: 0,
            api_limits: HashMap::new(),
            burst_size: 50,
            client_ip_qps: Some(100),
            client_ip_burst: Some(50),
            enabled: true,
            expected_updated_at: None,
        }
    }

    #[test]
    fn plan_qps_fields_validate_ok() {
        assert!(validate_ratelimit_config(&base_plan()).is_ok());
    }

    #[test]
    fn api_override_cannot_exceed_plan_qps() {
        let mut cfg = base_plan();
        cfg.api_limits.insert("/posts".into(), 200);
        let err = validate_ratelimit_config(&cfg).unwrap_err();
        assert!(err.contains("plan QPS"), "err={err}");
    }

    #[test]
    fn api_override_within_plan_ok() {
        let mut cfg = base_plan();
        cfg.api_limits.insert("/posts".into(), 40);
        assert!(validate_ratelimit_config(&cfg).is_ok());
    }
}
