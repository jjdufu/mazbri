use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

macro_rules! profile_returning {
    () => {
        "tenant_id, encryption_mode, suite, data_alg, mac_alg,          require_signature, allow_insecure, nonce_ttl_seconds, clock_skew_seconds,          config, created_at, updated_at"
    };
}

macro_rules! key_returning {
    () => {
        "id, tenant_id, encrypted_secret_key, secret_key_digest, wrapping_key_id,          wrapping_algorithm, last_used_at, created_at, updated_at, metadata"
    };
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TenantCryptoProfileRow {
    pub tenant_id: uuid::Uuid,
    pub encryption_mode: String,
    pub suite: Option<String>,
    pub data_alg: Option<String>,
    pub mac_alg: Option<String>,
    pub require_signature: bool,
    pub allow_insecure: bool,
    pub nonce_ttl_seconds: i32,
    pub clock_skew_seconds: i32,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TenantCryptoKeyRow {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    #[serde(skip_serializing)]
    pub encrypted_secret_key: String,
    pub secret_key_digest: String,
    pub wrapping_key_id: String,
    pub wrapping_algorithm: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TenantCryptoKeyEventRow {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub key_id: Option<uuid::Uuid>,
    pub event_type: String,
    pub actor_type: String,
    pub actor_id: Option<uuid::Uuid>,
    pub request_id: Option<String>,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// 读取租户 Profile；不存在时插入默认行。
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn upsert_profile_defaults(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<TenantCryptoProfileRow, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoProfileRow>(concat!(
        "WITH inserted AS (             INSERT INTO tenant_crypto_profiles (tenant_id)             VALUES ($1)             ON CONFLICT (tenant_id) DO NOTHING             RETURNING ",
        profile_returning!(),
        ")          SELECT ",
        profile_returning!(),
        " FROM inserted          UNION ALL          SELECT ",
        profile_returning!(),
        " FROM tenant_crypto_profiles          WHERE tenant_id = $1 AND NOT EXISTS (SELECT 1 FROM inserted)"
    ))
    .bind(tenant_id)
    .fetch_one(pool)
    .await
}

pub struct UpdateCryptoProfileParams<'a> {
    pub tenant_id: uuid::Uuid,
    pub encryption_mode: &'a str,
    pub suite: Option<&'a str>,
    pub data_alg: Option<&'a str>,
    pub mac_alg: Option<&'a str>,
    pub require_signature: bool,
    pub allow_insecure: bool,
    pub nonce_ttl_seconds: i32,
    pub clock_skew_seconds: i32,
    pub actor_id: uuid::Uuid,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_profile(
    pool: &PgPool,
    params: UpdateCryptoProfileParams<'_>,
) -> Result<TenantCryptoProfileRow, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoProfileRow>(concat!(
        "INSERT INTO tenant_crypto_profiles (             tenant_id, encryption_mode, suite, data_alg, mac_alg,             require_signature, allow_insecure, nonce_ttl_seconds, clock_skew_seconds,             created_by, updated_by         )         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)         ON CONFLICT (tenant_id) DO UPDATE SET             encryption_mode = EXCLUDED.encryption_mode,             suite = EXCLUDED.suite,             data_alg = EXCLUDED.data_alg,             mac_alg = EXCLUDED.mac_alg,             require_signature = EXCLUDED.require_signature,             allow_insecure = EXCLUDED.allow_insecure,             nonce_ttl_seconds = EXCLUDED.nonce_ttl_seconds,             clock_skew_seconds = EXCLUDED.clock_skew_seconds,             updated_by = EXCLUDED.updated_by,             updated_at = NOW()         RETURNING ",
        profile_returning!()
    ))
    .bind(params.tenant_id)
    .bind(params.encryption_mode)
    .bind(params.suite)
    .bind(params.data_alg)
    .bind(params.mac_alg)
    .bind(params.require_signature)
    .bind(params.allow_insecure)
    .bind(params.nonce_ttl_seconds)
    .bind(params.clock_skew_seconds)
    .bind(params.actor_id)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn find_key(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<Option<TenantCryptoKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoKeyRow>(concat!(
        "SELECT ",
        key_returning!(),
        " FROM tenant_crypto_keys WHERE tenant_id = $1"
    ))
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
}

pub struct UpsertCryptoKeyParams<'a> {
    pub tenant_id: uuid::Uuid,
    pub encrypted_secret_key: &'a str,
    pub secret_key_digest: &'a str,
    pub wrapping_key_id: &'a str,
    pub actor_id: uuid::Uuid,
    pub metadata: &'a serde_json::Value,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn log_key_event(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    key_id: Option<uuid::Uuid>,
    event_type: &str,
    actor_id: uuid::Uuid,
    details: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        INSERT INTO tenant_crypto_key_events (
            tenant_id, key_id, event_type, actor_type, actor_id, details
        )
        VALUES ($1, $2, $3, 'admin', $4, $5)
        ",
    )
    .bind(tenant_id)
    .bind(key_id)
    .bind(event_type)
    .bind(actor_id)
    .bind(details)
    .execute(pool)
    .await?;
    Ok(())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_all_profiles(pool: &PgPool) -> Result<Vec<TenantCryptoProfileRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoProfileRow>(concat!(
        "SELECT ",
        profile_returning!(),
        " FROM tenant_crypto_profiles ORDER BY tenant_id"
    ))
    .fetch_all(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_all_keys(pool: &PgPool) -> Result<Vec<TenantCryptoKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoKeyRow>(concat!(
        "SELECT ",
        key_returning!(),
        " FROM tenant_crypto_keys ORDER BY tenant_id"
    ))
    .fetch_all(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_all_key_events(
    pool: &PgPool,
) -> Result<Vec<TenantCryptoKeyEventRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoKeyEventRow>(
        r"
        SELECT id, tenant_id, key_id, event_type, actor_type, actor_id,
            request_id, details, created_at
        FROM tenant_crypto_key_events
        ORDER BY tenant_id, created_at
        ",
    )
    .fetch_all(pool)
    .await
}

/// 在单事务内锁定租户行（防并发密钥创建/轮换竞态）。
///
/// # Errors
///
/// 底层数据库查询失败时返回错误。
pub async fn lock_tenant_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: uuid::Uuid,
) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM tenants WHERE id = $1 FOR UPDATE")
        .bind(tenant_id)
        .fetch_optional(&mut **tx)
        .await
}

/// 在单事务内读取或初始化租户加密 Profile。
///
/// # Errors
///
/// 底层数据库查询失败时返回错误。
pub async fn upsert_profile_defaults_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: uuid::Uuid,
) -> Result<TenantCryptoProfileRow, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoProfileRow>(concat!(
        "WITH inserted AS (             INSERT INTO tenant_crypto_profiles (tenant_id)             VALUES ($1)             ON CONFLICT (tenant_id) DO NOTHING             RETURNING ",
        profile_returning!(),
        ")          SELECT ",
        profile_returning!(),
        " FROM inserted          UNION ALL          SELECT ",
        profile_returning!(),
        " FROM tenant_crypto_profiles          WHERE tenant_id = $1 AND NOT EXISTS (SELECT 1 FROM inserted)"
    ))
    .bind(tenant_id)
    .fetch_one(&mut **tx)
    .await
}

/// 在单事务内查找租户主密钥。
///
/// # Errors
///
/// 底层数据库查询失败时返回错误。
pub async fn find_key_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: uuid::Uuid,
) -> Result<Option<TenantCryptoKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoKeyRow>(concat!(
        "SELECT ",
        key_returning!(),
        " FROM tenant_crypto_keys WHERE tenant_id = $1"
    ))
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await
}

/// 在单事务内插入或轮换替换租户唯一主密钥。
///
/// # Errors
///
/// 底层数据库查询失败时返回错误。
pub async fn upsert_key_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    params: UpsertCryptoKeyParams<'_>,
) -> Result<TenantCryptoKeyRow, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoKeyRow>(concat!(
        "INSERT INTO tenant_crypto_keys (             tenant_id, encrypted_secret_key, secret_key_digest, wrapping_key_id,             created_by, updated_by, metadata         )         VALUES ($1, $2, $3, $4, $5, $5, $6)         ON CONFLICT (tenant_id) DO UPDATE SET             encrypted_secret_key = EXCLUDED.encrypted_secret_key,             secret_key_digest = EXCLUDED.secret_key_digest,             wrapping_key_id = EXCLUDED.wrapping_key_id,             updated_by = EXCLUDED.updated_by,             updated_at = NOW(),             metadata = EXCLUDED.metadata         RETURNING ",
        key_returning!()
    ))
    .bind(params.tenant_id)
    .bind(params.encrypted_secret_key)
    .bind(params.secret_key_digest)
    .bind(params.wrapping_key_id)
    .bind(params.actor_id)
    .bind(params.metadata)
    .fetch_one(&mut **tx)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_crypto_key_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: uuid::Uuid,
) -> Result<Option<TenantCryptoKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoKeyRow>(concat!(
        "DELETE FROM tenant_crypto_keys WHERE tenant_id = $1 RETURNING ",
        key_returning!()
    ))
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn log_key_event_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: uuid::Uuid,
    key_id: Option<uuid::Uuid>,
    event_type: &str,
    actor_id: uuid::Uuid,
    details: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        INSERT INTO tenant_crypto_key_events (
            tenant_id, key_id, event_type, actor_type, actor_id, details
        )
        VALUES ($1, $2, $3, 'admin', $4, $5)
        ",
    )
    .bind(tenant_id)
    .bind(key_id)
    .bind(event_type)
    .bind(actor_id)
    .bind(details)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
