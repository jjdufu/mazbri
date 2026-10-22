use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RateLimitConfigRow {
    pub tenant_id: uuid::Uuid,
    /// Plan QPS mapped from DB `tenant_qps` (API wire name: `max_concurrent`, alias `tenant_qps`).
    pub max_concurrent: i32,
    pub max_bandwidth_mbps: i32,
    pub api_limits: serde_json::Value,
    pub burst_size: i32,
    pub client_ip_qps: i32,
    pub client_ip_burst: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CacheConfigRow {
    pub tenant_id: uuid::Uuid,
    pub enabled: bool,
    pub default_ttl: i32,
    pub max_cache_size: i32,
    pub api_ttl: serde_json::Value,
    pub tenant_isolated: bool,
    pub warmup_enabled: bool,
    pub warmup_keys: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub use super::tenant_helpers::tenant_exists;

const RATE_LIMIT_SELECT: &str = r"
    tenant_id,
    tenant_qps AS max_concurrent,
    (bandwidth_bps / 1000000)::integer AS max_bandwidth_mbps,
    api_limits,
    tenant_burst AS burst_size,
    client_ip_qps,
    client_ip_burst,
    enabled,
    created_at,
    updated_at
";

/// Write payload for tenant rate-limit policy rows.
#[derive(Debug, Clone)]
pub struct RateLimitWriteParams<'a> {
    pub max_concurrent: i32,
    pub max_bandwidth_mbps: i32,
    pub api_limits: &'a serde_json::Value,
    pub burst_size: i32,
    pub client_ip_qps: i32,
    pub client_ip_burst: i32,
    pub enabled: bool,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_ratelimit_config(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<Option<RateLimitConfigRow>, sqlx::Error> {
    let sql =
        format!("SELECT {RATE_LIMIT_SELECT} FROM rate_limit_tenant_policies WHERE tenant_id = $1");
    sqlx::query_as::<_, RateLimitConfigRow>(&sql)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn upsert_ratelimit_config(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    params: &RateLimitWriteParams<'_>,
) -> Result<RateLimitConfigRow, sqlx::Error> {
    let sql = format!(
        r"
        INSERT INTO rate_limit_tenant_policies (
            tenant_id, tenant_qps, tenant_burst,
            bandwidth_bps, api_limits,
            client_ip_qps, client_ip_burst, enabled
        )
        VALUES (
            $1, $2, $3,
            ($4::bigint * 1000000), $5,
            $6, $7, $8
        )
        ON CONFLICT (tenant_id) DO UPDATE SET
            tenant_qps = EXCLUDED.tenant_qps,
            tenant_burst = EXCLUDED.tenant_burst,
            bandwidth_bps = EXCLUDED.bandwidth_bps,
            api_limits = EXCLUDED.api_limits,
            client_ip_qps = EXCLUDED.client_ip_qps,
            client_ip_burst = EXCLUDED.client_ip_burst,
            enabled = EXCLUDED.enabled,
            updated_at = NOW()
        RETURNING {RATE_LIMIT_SELECT}
        "
    );
    sqlx::query_as::<_, RateLimitConfigRow>(&sql)
        .bind(tenant_id)
        .bind(params.max_concurrent)
        .bind(params.burst_size)
        .bind(params.max_bandwidth_mbps)
        .bind(params.api_limits)
        .bind(params.client_ip_qps)
        .bind(params.client_ip_burst)
        .bind(params.enabled)
        .fetch_one(pool)
        .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_ratelimit_config(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    params: &RateLimitWriteParams<'_>,
) -> Result<Option<RateLimitConfigRow>, sqlx::Error> {
    let sql = format!(
        r"
        INSERT INTO rate_limit_tenant_policies (
            tenant_id, tenant_qps, tenant_burst,
            bandwidth_bps, api_limits,
            client_ip_qps, client_ip_burst, enabled
        )
        VALUES (
            $1, $2, $3,
            ($4::bigint * 1000000), $5,
            $6, $7, $8
        )
        ON CONFLICT (tenant_id) DO NOTHING
        RETURNING {RATE_LIMIT_SELECT}
        "
    );
    sqlx::query_as::<_, RateLimitConfigRow>(&sql)
        .bind(tenant_id)
        .bind(params.max_concurrent)
        .bind(params.burst_size)
        .bind(params.max_bandwidth_mbps)
        .bind(params.api_limits)
        .bind(params.client_ip_qps)
        .bind(params.client_ip_burst)
        .bind(params.enabled)
        .fetch_optional(pool)
        .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_ratelimit_config_if_unchanged(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    params: &RateLimitWriteParams<'_>,
    expected_updated_at: DateTime<Utc>,
) -> Result<Option<RateLimitConfigRow>, sqlx::Error> {
    let sql = format!(
        r"
        UPDATE rate_limit_tenant_policies
        SET tenant_qps = $2,
            tenant_burst = $3,
            bandwidth_bps = ($4::bigint * 1000000),
            api_limits = $5,
            client_ip_qps = $6,
            client_ip_burst = $7,
            enabled = $8,
            updated_at = NOW()
        WHERE tenant_id = $1 AND updated_at = $9
        RETURNING {RATE_LIMIT_SELECT}
        "
    );
    sqlx::query_as::<_, RateLimitConfigRow>(&sql)
        .bind(tenant_id)
        .bind(params.max_concurrent)
        .bind(params.burst_size)
        .bind(params.max_bandwidth_mbps)
        .bind(params.api_limits)
        .bind(params.client_ip_qps)
        .bind(params.client_ip_burst)
        .bind(params.enabled)
        .bind(expected_updated_at)
        .fetch_optional(pool)
        .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_enabled_ratelimit_configs(
    pool: &PgPool,
) -> Result<Vec<RateLimitConfigRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {RATE_LIMIT_SELECT} FROM rate_limit_tenant_policies WHERE enabled = true ORDER BY updated_at DESC LIMIT 100"
    );
    sqlx::query_as::<_, RateLimitConfigRow>(&sql)
        .fetch_all(pool)
        .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_ratelimit_config(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM rate_limit_tenant_policies WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Write payload for tenant cache config rows.
#[derive(Debug, Clone)]
pub struct CacheWriteParams<'a> {
    pub enabled: bool,
    pub default_ttl: i32,
    pub max_cache_size: i32,
    pub api_ttl: &'a serde_json::Value,
    pub tenant_isolated: bool,
    pub warmup_enabled: bool,
    pub warmup_keys: &'a [String],
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_cache_config(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<Option<CacheConfigRow>, sqlx::Error> {
    sqlx::query_as::<_, CacheConfigRow>(
        r"
        SELECT tenant_id, enabled, default_ttl, max_cache_size, api_ttl,
               tenant_isolated, warmup_enabled, warmup_keys, created_at, updated_at
        FROM cache_configs WHERE tenant_id = $1
    ",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn upsert_cache_config(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    params: &CacheWriteParams<'_>,
) -> Result<CacheConfigRow, sqlx::Error> {
    sqlx::query_as::<_, CacheConfigRow>(
        r"
        INSERT INTO cache_configs (tenant_id, enabled, default_ttl, max_cache_size, api_ttl,
            tenant_isolated, warmup_enabled, warmup_keys)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (tenant_id) DO UPDATE SET
            enabled = EXCLUDED.enabled, default_ttl = EXCLUDED.default_ttl,
            max_cache_size = EXCLUDED.max_cache_size, api_ttl = EXCLUDED.api_ttl,
            tenant_isolated = EXCLUDED.tenant_isolated, warmup_enabled = EXCLUDED.warmup_enabled,
            warmup_keys = EXCLUDED.warmup_keys, updated_at = NOW()
        RETURNING tenant_id, enabled, default_ttl, max_cache_size, api_ttl,
                  tenant_isolated, warmup_enabled, warmup_keys, created_at, updated_at
    ",
    )
    .bind(tenant_id)
    .bind(params.enabled)
    .bind(params.default_ttl)
    .bind(params.max_cache_size)
    .bind(params.api_ttl)
    .bind(params.tenant_isolated)
    .bind(params.warmup_enabled)
    .bind(params.warmup_keys)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_cache_config(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    params: &CacheWriteParams<'_>,
) -> Result<Option<CacheConfigRow>, sqlx::Error> {
    sqlx::query_as::<_, CacheConfigRow>(
        r"
        INSERT INTO cache_configs (tenant_id, enabled, default_ttl, max_cache_size, api_ttl,
            tenant_isolated, warmup_enabled, warmup_keys)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (tenant_id) DO NOTHING
        RETURNING tenant_id, enabled, default_ttl, max_cache_size, api_ttl,
                  tenant_isolated, warmup_enabled, warmup_keys, created_at, updated_at
    ",
    )
    .bind(tenant_id)
    .bind(params.enabled)
    .bind(params.default_ttl)
    .bind(params.max_cache_size)
    .bind(params.api_ttl)
    .bind(params.tenant_isolated)
    .bind(params.warmup_enabled)
    .bind(params.warmup_keys)
    .fetch_optional(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_cache_config_if_unchanged(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    params: &CacheWriteParams<'_>,
    expected_updated_at: DateTime<Utc>,
) -> Result<Option<CacheConfigRow>, sqlx::Error> {
    sqlx::query_as::<_, CacheConfigRow>(
        r"
        UPDATE cache_configs
        SET enabled = $2,
            default_ttl = $3,
            max_cache_size = $4,
            api_ttl = $5,
            tenant_isolated = $6,
            warmup_enabled = $7,
            warmup_keys = $8,
            updated_at = NOW()
        WHERE tenant_id = $1 AND updated_at = $9
        RETURNING tenant_id, enabled, default_ttl, max_cache_size, api_ttl,
                  tenant_isolated, warmup_enabled, warmup_keys, created_at, updated_at
    ",
    )
    .bind(tenant_id)
    .bind(params.enabled)
    .bind(params.default_ttl)
    .bind(params.max_cache_size)
    .bind(params.api_ttl)
    .bind(params.tenant_isolated)
    .bind(params.warmup_enabled)
    .bind(params.warmup_keys)
    .bind(expected_updated_at)
    .fetch_optional(pool)
    .await
}
