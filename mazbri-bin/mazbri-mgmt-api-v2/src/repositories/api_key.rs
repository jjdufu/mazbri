use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ApiKeyRow {
    pub id: uuid::Uuid,
    pub prefix: String,
    pub scopes: Vec<String>,
    pub is_revoked: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}
#[derive(Debug, Serialize)]
pub struct ApiKeyListRow {
    pub prefix: String,
    pub scopes: Vec<String>,
    pub is_revoked: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub key_suffix: Option<String>,
    pub is_active: bool,
}

pub use super::tenant_helpers::tenant_exists;
pub async fn count_active_api_keys(
    auth_pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM api_keys ak
        JOIN auth_credentials ac ON ak.credential_id = ac.id
        WHERE ac.tenant_id = $1
          AND ac.credential_type = 'api_key'
          AND ac.is_active = true
          AND ak.is_active = true
          AND ak.is_revoked = false
        "#,
    )
    .bind(tenant_id)
    .fetch_one(auth_pool)
    .await
}
pub async fn insert_credential(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    credential_id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    prefix: &str,
    secret_hash: &str,
    expires_at: DateTime<Utc>,
    metadata_json: serde_json::Value,
) -> Result<uuid::Uuid, sqlx::Error> {
    let row = sqlx::query(
            r#"
        INSERT INTO auth_credentials (id, tenant_id, credential_type, credential_id, credential_secret_hash, is_active, expires_at, metadata)
        VALUES ($1, $2, 'api_key', $3, $4, true, $5, $6)
        RETURNING id
    "#,
        )
        .bind(credential_id)
        .bind(tenant_id)
        .bind(prefix)
        .bind(secret_hash)
        .bind(expires_at)
        .bind(metadata_json)
        .fetch_one(&mut **tx)
        .await?;
    Ok(row.get("id"))
}
pub struct InsertApiKeyParams<'a> {
    pub api_key_id: uuid::Uuid,
    pub credential_id: uuid::Uuid,
    pub prefix: &'a str,
    pub key_hash: &'a str,
    pub scopes: &'a [String],
    pub expires_at: DateTime<Utc>,
    pub key_suffix: &'a str,
}

pub async fn insert_api_key(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    params: InsertApiKeyParams<'_>,
) -> Result<ApiKeyRow, sqlx::Error> {
    sqlx::query_as::<_, ApiKeyRow>(
        r#"
        INSERT INTO api_keys (id, credential_id, prefix, key_hash, scopes, expires_at, key_suffix)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, prefix, scopes, is_revoked, created_at, expires_at
    "#,
    )
    .bind(params.api_key_id)
    .bind(params.credential_id)
    .bind(params.prefix)
    .bind(params.key_hash)
    .bind(params.scopes)
    .bind(params.expires_at)
    .bind(params.key_suffix)
    .fetch_one(&mut **tx)
    .await
}
pub async fn list_api_keys(
    auth_pool: &PgPool,
    tenant_id: uuid::Uuid,
) -> Result<Vec<ApiKeyListRow>, sqlx::Error> {
    let query = r#"
        SELECT
            ak.prefix,
            ak.scopes,
            ak.is_revoked,
            ak.is_active AS api_key_active,
            ak.created_at,
            ak.expires_at,
            ak.key_suffix,
            ac.is_active
        FROM api_keys ak
        JOIN auth_credentials ac ON ak.credential_id = ac.id
        WHERE ac.tenant_id = $1 AND ac.credential_type = 'api_key' AND ak.is_revoked = false
        ORDER BY ak.created_at DESC
    "#;
    let rows = sqlx::query(query)
        .bind(tenant_id)
        .fetch_all(auth_pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| ApiKeyListRow {
            prefix: row.get("prefix"),
            scopes: row.get("scopes"),
            is_revoked: row.get("is_revoked"),
            created_at: row.get("created_at"),
            expires_at: row.get("expires_at"),
            key_suffix: row.get("key_suffix"),
            is_active: row.get::<bool, _>("is_active")
                && row.get::<bool, _>("api_key_active")
                && !row.get::<bool, _>("is_revoked"),
        })
        .collect())
}
pub async fn revoke_api_key_by_prefix(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    prefix: &str,
) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        UPDATE api_keys
        SET is_active = false, is_revoked = true, revoked_at = NOW()
        WHERE prefix = $1
        RETURNING id
    "#,
    )
    .bind(prefix)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|r| r.get("id")))
}
pub async fn deactivate_credential_by_prefix(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    prefix: &str,
) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        UPDATE auth_credentials
        SET is_active = false
        WHERE credential_type = 'api_key' AND credential_id = $1
        RETURNING id
    "#,
    )
    .bind(prefix)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|r| r.get("id")))
}
pub async fn get_api_key_expiration(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    prefix: &str,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    sqlx::query_scalar("SELECT expires_at FROM api_keys WHERE prefix = $1")
        .bind(prefix)
        .fetch_optional(&mut **tx)
        .await
}
pub async fn update_api_key_expiration(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    prefix: &str,
    expires_at: DateTime<Utc>,
) -> Result<Option<ApiKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, ApiKeyRow>(
        r#"
        UPDATE api_keys
        SET expires_at = $2
        WHERE prefix = $1
        RETURNING id, prefix, scopes, is_revoked, created_at, expires_at
    "#,
    )
    .bind(prefix)
    .bind(expires_at)
    .fetch_optional(&mut **tx)
    .await
}
pub async fn update_credential_expiration(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    prefix: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let query = r#"
        UPDATE auth_credentials
        SET expires_at = $2
        WHERE credential_type = 'api_key' AND credential_id = $1
    "#;
    sqlx::query(query)
        .bind(prefix)
        .bind(expires_at)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
