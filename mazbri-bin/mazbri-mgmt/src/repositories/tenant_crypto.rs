use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TenantCryptoProfileRow {
    pub tenant_id: uuid::Uuid,
    pub encryption_mode: String,
    pub require_signature: bool,
    pub require_request_binding: bool,
    pub signature_version: String,
    pub request_algorithm: String,
    pub response_algorithm: String,
    pub signature_algorithm: String,
    pub current_key_version: Option<String>,
    pub nonce_ttl_seconds: i32,
    pub clock_skew_seconds: i32,
    pub request_codec: String,
    pub response_codec: String,
    pub codec_enabled: bool,
    pub codec_version: i32,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TenantCryptoKeyRow {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub key_version: String,
    #[serde(skip_serializing)]
    pub encrypted_secret_key: String,
    pub secret_key_digest: String,
    pub wrapping_key_id: String,
    pub wrapping_algorithm: String,
    pub data_algorithm: String,
    pub signature_algorithm: String,
    pub status: String,
    pub not_before: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub activated_at: Option<DateTime<Utc>>,
    pub deprecated_at: Option<DateTime<Utc>>,
    pub disabled_at: Option<DateTime<Utc>>,
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
    pub key_version: Option<String>,
    pub event_type: String,
    pub actor_type: String,
    pub actor_id: Option<uuid::Uuid>,
    pub request_id: Option<String>,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn upsert_profile_defaults(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<TenantCryptoProfileRow, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoProfileRow>(
        r"
        INSERT INTO tenant_crypto_profiles (tenant_id)
        VALUES ($1)
        ON CONFLICT (tenant_id) DO UPDATE SET updated_at = tenant_crypto_profiles.updated_at
        RETURNING tenant_id, encryption_mode, require_signature, require_request_binding, signature_version,
            request_algorithm, response_algorithm,
            signature_algorithm, current_key_version, nonce_ttl_seconds, clock_skew_seconds,
            request_codec, response_codec, codec_enabled, codec_version, config, created_at, updated_at
        ",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
}

pub struct UpdateCryptoProfileParams<'a> {
    pub tenant_id: uuid::Uuid,
    pub encryption_mode: &'a str,
    pub require_signature: bool,
    pub require_request_binding: bool,
    pub signature_version: &'a str,
    pub nonce_ttl_seconds: i32,
    pub clock_skew_seconds: i32,
    pub current_key_version: Option<&'a str>,
    pub request_codec: &'a str,
    pub response_codec: &'a str,
    pub codec_enabled: bool,
    pub codec_version: i32,
    pub actor_id: uuid::Uuid,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_profile(
    pool: &PgPool,
    params: UpdateCryptoProfileParams<'_>,
) -> Result<TenantCryptoProfileRow, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoProfileRow>(
        r"
        INSERT INTO tenant_crypto_profiles (
            tenant_id, encryption_mode, require_signature, require_request_binding, signature_version,
            nonce_ttl_seconds,
            clock_skew_seconds, current_key_version, request_codec, response_codec,
            codec_enabled, codec_version, created_by, updated_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)
        ON CONFLICT (tenant_id) DO UPDATE SET
            encryption_mode = EXCLUDED.encryption_mode,
            require_signature = EXCLUDED.require_signature,
            require_request_binding = EXCLUDED.require_request_binding,
            signature_version = EXCLUDED.signature_version,
            nonce_ttl_seconds = EXCLUDED.nonce_ttl_seconds,
            clock_skew_seconds = EXCLUDED.clock_skew_seconds,
            current_key_version = EXCLUDED.current_key_version,
            request_codec = EXCLUDED.request_codec,
            response_codec = EXCLUDED.response_codec,
            codec_enabled = EXCLUDED.codec_enabled,
            codec_version = EXCLUDED.codec_version,
            updated_by = EXCLUDED.updated_by,
            updated_at = NOW()
        RETURNING tenant_id, encryption_mode, require_signature, require_request_binding, signature_version,
            request_algorithm, response_algorithm,
            signature_algorithm, current_key_version, nonce_ttl_seconds, clock_skew_seconds,
            request_codec, response_codec, codec_enabled, codec_version, config, created_at, updated_at
        ",
    )
    .bind(params.tenant_id)
    .bind(params.encryption_mode)
    .bind(params.require_signature)
    .bind(params.require_request_binding)
    .bind(params.signature_version)
    .bind(params.nonce_ttl_seconds)
    .bind(params.clock_skew_seconds)
    .bind(params.current_key_version)
    .bind(params.request_codec)
    .bind(params.response_codec)
    .bind(params.codec_enabled)
    .bind(params.codec_version)
    .bind(params.actor_id)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_keys(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<Vec<TenantCryptoKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoKeyRow>(
        r"
        SELECT id, tenant_id, key_version, encrypted_secret_key, secret_key_digest, wrapping_key_id,
            wrapping_algorithm, data_algorithm, signature_algorithm, status, not_before, expires_at,
            activated_at, deprecated_at, disabled_at, last_used_at, created_at, updated_at, metadata
        FROM tenant_crypto_keys
        WHERE tenant_id = $1
        ORDER BY created_at DESC
        ",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn find_key(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    key_version: &str,
) -> Result<Option<TenantCryptoKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoKeyRow>(
        r"
        SELECT id, tenant_id, key_version, encrypted_secret_key, secret_key_digest, wrapping_key_id,
            wrapping_algorithm, data_algorithm, signature_algorithm, status, not_before, expires_at,
            activated_at, deprecated_at, disabled_at, last_used_at, created_at, updated_at, metadata
        FROM tenant_crypto_keys
        WHERE tenant_id = $1 AND key_version = $2
        ",
    )
    .bind(tenant_id)
    .bind(key_version)
    .fetch_optional(pool)
    .await
}

pub struct InsertCryptoKeyParams<'a> {
    pub tenant_id: uuid::Uuid,
    pub key_version: &'a str,
    pub encrypted_secret_key: &'a str,
    pub secret_key_digest: &'a str,
    pub wrapping_key_id: &'a str,
    pub status: &'a str,
    pub actor_id: uuid::Uuid,
    pub metadata: &'a serde_json::Value,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_key(
    pool: &PgPool,
    params: InsertCryptoKeyParams<'_>,
) -> Result<TenantCryptoKeyRow, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoKeyRow>(
        r"
        INSERT INTO tenant_crypto_keys (
            tenant_id, key_version, encrypted_secret_key, secret_key_digest, wrapping_key_id,
            status, activated_at, created_by, updated_by, metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, CASE WHEN $6 = 'active' THEN NOW() ELSE NULL END, $7, $7, $8)
        RETURNING id, tenant_id, key_version, encrypted_secret_key, secret_key_digest, wrapping_key_id,
            wrapping_algorithm, data_algorithm, signature_algorithm, status, not_before, expires_at,
            activated_at, deprecated_at, disabled_at, last_used_at, created_at, updated_at, metadata
        ",
    )
    .bind(params.tenant_id)
    .bind(params.key_version)
    .bind(params.encrypted_secret_key)
    .bind(params.secret_key_digest)
    .bind(params.wrapping_key_id)
    .bind(params.status)
    .bind(params.actor_id)
    .bind(params.metadata)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_key_status(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    key_version: &str,
    status: &str,
    actor_id: uuid::Uuid,
) -> Result<Option<TenantCryptoKeyRow>, sqlx::Error> {
    let timestamp_column = match status {
        "active" => "activated_at",
        "deprecated" => "deprecated_at",
        "disabled" => "disabled_at",
        _ => "updated_at",
    };
    let sql = format!(
        r"
        UPDATE tenant_crypto_keys
        SET status = $1, updated_by = $2, updated_at = NOW(), {timestamp_column} = NOW()
        WHERE tenant_id = $3 AND key_version = $4
        RETURNING id, tenant_id, key_version, encrypted_secret_key, secret_key_digest, wrapping_key_id,
            wrapping_algorithm, data_algorithm, signature_algorithm, status, not_before, expires_at,
            activated_at, deprecated_at, disabled_at, last_used_at, created_at, updated_at, metadata
        "
    );
    sqlx::query_as::<_, TenantCryptoKeyRow>(&sql)
        .bind(status)
        .bind(actor_id)
        .bind(tenant_id)
        .bind(key_version)
        .fetch_optional(pool)
        .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn deprecate_other_active_keys(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    except_version: &str,
    actor_id: uuid::Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        UPDATE tenant_crypto_keys
        SET status = 'deprecated', deprecated_at = COALESCE(deprecated_at, NOW()),
            updated_at = NOW(), updated_by = $3
        WHERE tenant_id = $1 AND key_version <> $2 AND status = 'active'
        ",
    )
    .bind(tenant_id)
    .bind(except_version)
    .bind(actor_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn set_current_key_version(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    key_version: Option<&str>,
    actor_id: uuid::Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        INSERT INTO tenant_crypto_profiles (tenant_id, current_key_version, created_by, updated_by)
        VALUES ($1, $2, $3, $3)
        ON CONFLICT (tenant_id) DO UPDATE SET
            current_key_version = EXCLUDED.current_key_version,
            updated_by = EXCLUDED.updated_by,
            updated_at = NOW()
        ",
    )
    .bind(tenant_id)
    .bind(key_version)
    .bind(actor_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn log_key_event(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    key_id: Option<uuid::Uuid>,
    key_version: Option<&str>,
    event_type: &str,
    actor_id: uuid::Uuid,
    details: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        INSERT INTO tenant_crypto_key_events (
            tenant_id, key_id, key_version, event_type, actor_type, actor_id, details
        )
        VALUES ($1, $2, $3, $4, 'admin', $5, $6)
        ",
    )
    .bind(tenant_id)
    .bind(key_id)
    .bind(key_version)
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
pub async fn active_key_count(pool: &PgPool, tenant_id: uuid::Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query(
        "SELECT COUNT(*) FROM tenant_crypto_keys WHERE tenant_id = $1 AND status = 'active'",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map(|row| row.get::<i64, _>(0))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_all_profiles(pool: &PgPool) -> Result<Vec<TenantCryptoProfileRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoProfileRow>(
        r"
        SELECT tenant_id, encryption_mode, require_signature, require_request_binding, signature_version,
            request_algorithm, response_algorithm,
            signature_algorithm, current_key_version, nonce_ttl_seconds, clock_skew_seconds,
            request_codec, response_codec, codec_enabled, codec_version, config, created_at,
            updated_at
        FROM tenant_crypto_profiles
        ORDER BY tenant_id
        ",
    )
    .fetch_all(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_all_keys(pool: &PgPool) -> Result<Vec<TenantCryptoKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoKeyRow>(
        r"
        SELECT id, tenant_id, key_version, encrypted_secret_key, secret_key_digest, wrapping_key_id,
            wrapping_algorithm, data_algorithm, signature_algorithm, status, not_before, expires_at,
            activated_at, deprecated_at, disabled_at, last_used_at, created_at, updated_at, metadata
        FROM tenant_crypto_keys
        ORDER BY tenant_id, key_version
        ",
    )
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
        SELECT id, tenant_id, key_id, key_version, event_type, actor_type, actor_id,
            request_id, details, created_at
        FROM tenant_crypto_key_events
        ORDER BY tenant_id, created_at
        ",
    )
    .fetch_all(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_crypto_key(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    key_version: &str,
) -> Result<Option<TenantCryptoKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoKeyRow>(
        r"
        DELETE FROM tenant_crypto_keys
        WHERE tenant_id = $1 AND key_version = $2
        RETURNING id, tenant_id, key_version, encrypted_secret_key, secret_key_digest, wrapping_key_id,
            wrapping_algorithm, data_algorithm, signature_algorithm, status, not_before, expires_at,
            activated_at, deprecated_at, disabled_at, last_used_at, created_at, updated_at, metadata
        "
    )
    .bind(tenant_id)
    .bind(key_version)
    .fetch_optional(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_crypto_key_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: uuid::Uuid,
    key_version: &str,
) -> Result<Option<TenantCryptoKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantCryptoKeyRow>(
        r"
        DELETE FROM tenant_crypto_keys
        WHERE tenant_id = $1 AND key_version = $2
        RETURNING id, tenant_id, key_version, encrypted_secret_key, secret_key_digest, wrapping_key_id,
            wrapping_algorithm, data_algorithm, signature_algorithm, status, not_before, expires_at,
            activated_at, deprecated_at, disabled_at, last_used_at, created_at, updated_at, metadata
        "
    )
    .bind(tenant_id)
    .bind(key_version)
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
    key_version: Option<&str>,
    event_type: &str,
    actor_id: uuid::Uuid,
    details: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        INSERT INTO tenant_crypto_key_events (
            tenant_id, key_id, key_version, event_type, actor_type, actor_id, details
        )
        VALUES ($1, $2, $3, $4, 'admin', $5, $6)
        ",
    )
    .bind(tenant_id)
    .bind(key_id)
    .bind(key_version)
    .bind(event_type)
    .bind(actor_id)
    .bind(details)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TenantRequestKeyRow {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub kid: String,
    pub alg: String,
    pub status: String,
    pub not_before: Option<DateTime<Utc>>,
    pub not_after: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_request_keys(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<Vec<TenantRequestKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantRequestKeyRow>(
        r"
        SELECT id, tenant_id, kid, alg, status, not_before, not_after, created_at, updated_at
        FROM tenant_request_keys
        WHERE tenant_id = $1
        ORDER BY created_at
        ",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
}

/// `insert_request_key` 要写入的字段。
pub struct NewRequestKey<'a> {
    pub tenant_id: uuid::Uuid,
    pub kid: &'a str,
    pub alg: &'a str,
    pub encrypted_material: &'a str,
    pub material_digest: &'a str,
    pub wrapping_key_id: &'a str,
    pub actor_id: uuid::Uuid,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_request_key(
    pool: &PgPool,
    params: NewRequestKey<'_>,
) -> Result<TenantRequestKeyRow, sqlx::Error> {
    let NewRequestKey {
        tenant_id,
        kid,
        alg,
        encrypted_material,
        material_digest,
        wrapping_key_id,
        actor_id,
    } = params;
    sqlx::query_as::<_, TenantRequestKeyRow>(
        r"
        INSERT INTO tenant_request_keys (
            tenant_id, kid, alg, encrypted_material, material_digest, wrapping_key_id,
            status, created_by, updated_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'active', $7, $7)
        RETURNING id, tenant_id, kid, alg, status, not_before, not_after, created_at, updated_at
        ",
    )
    .bind(tenant_id)
    .bind(kid)
    .bind(alg)
    .bind(encrypted_material)
    .bind(material_digest)
    .bind(wrapping_key_id)
    .bind(actor_id)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn set_request_key_status(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    kid: &str,
    status: &str,
    actor_id: uuid::Uuid,
) -> Result<Option<TenantRequestKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, TenantRequestKeyRow>(
        r"
        UPDATE tenant_request_keys
        SET status = $3, updated_by = $4, updated_at = NOW()
        WHERE tenant_id = $1 AND kid = $2
        RETURNING id, tenant_id, kid, alg, status, not_before, not_after, created_at, updated_at
        ",
    )
    .bind(tenant_id)
    .bind(kid)
    .bind(status)
    .bind(actor_id)
    .fetch_optional(pool)
    .await
}
