use crate::audit::{AuditAction, AuditService, api_key_audit};
use crate::error::ApiError;
use crate::handlers::types::{CreateApiKeyRequest, UpdateApiKeyRequest};
use crate::repositories::api_key as repo;
use ::chrono::TimeDelta;
use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHasher};
use chrono::{DateTime, Utc};
use rand::RngExt;
use rand::distr::Alphanumeric;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tracing::info;
fn default_expires_timestamp() -> DateTime<Utc> {
    Utc::now() + TimeDelta::days(365)
}
fn parse_expiration(expires_at: Option<i64>, expires_days: Option<i64>) -> DateTime<Utc> {
    if let Some(timestamp) = expires_at
        && let Some(dt) = DateTime::<Utc>::from_timestamp(timestamp, 0)
    {
        return dt;
    }
    if let Some(days) = expires_days {
        return Utc::now() + TimeDelta::days(days);
    }
    default_expires_timestamp()
}
fn generate_api_key_material() -> Result<(String, String, String, String, String), ApiError> {
    let prefix: String = rand::rng()
        .sample_iter(Alphanumeric)
        .take(16)
        .map(char::from)
        .collect::<String>()
        .to_lowercase();
    let random_bytes: [u8; 32] = rand::random();
    let secret: String = hex::encode(random_bytes);
    let full_api_key = format!("mazbri_{prefix}_{secret}");
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2.hash_password(secret.as_bytes(), &salt);
    let secret_hash = password_hash
        .map_err(|e| ApiError::internal(format!("Failed to hash API key: {e}")))?
        .to_string();
    let key_hash = format!("{:x}", Sha256::digest(secret.as_bytes()));
    let key_suffix = secret
        .chars()
        .rev()
        .take(8)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    Ok((full_api_key, prefix, secret_hash, key_hash, key_suffix))
}
fn mask_api_key(prefix: &str, suffix: &str) -> String {
    if suffix == "****" || suffix.is_empty() {
        format!("mazbri_{prefix}...")
    } else {
        format!("mazbri_{prefix}_****{suffix}")
    }
}
async fn ensure_tenant_exists(
    db: &PgPool,
    tenant_id: uuid::Uuid,
    id_str: &str,
) -> Result<(), ApiError> {
    if !repo::tenant_exists(db, tenant_id).await {
        return Err(ApiError::not_found(
            "tenant".to_string(),
            id_str.to_string(),
        ));
    }
    Ok(())
}
/// 创建前的不写库检查。失败时调用方不得消费 fresh。
///
/// # Errors
///
/// 租户不存在、scopes 为空、已有激活密钥或数据库查询失败时返回错误。
pub async fn precheck_create_api_key(
    db: &PgPool,
    auth_db: &PgPool,
    id_or_code: &str,
    scopes: &[crate::handlers::types::ApiScope],
) -> Result<(), ApiError> {
    let tenant_id = crate::services::tenant::resolve_tenant_id(db, id_or_code).await?;
    ensure_tenant_exists(db, tenant_id, id_or_code).await?;
    if scopes.is_empty() {
        return Err(ApiError::validation("scopes 不能为空".to_string()));
    }
    let existing_count = repo::count_active_api_keys(auth_db, tenant_id)
        .await
        .map_err(|error| {
            tracing::error!("failed to count active API keys: {error}");
            ApiError::internal("检查 API 密钥失败".to_string())
        })?;
    if existing_count > 0 {
        return Err(ApiError::validation(
            "该租户已存在 API 密钥，请使用轮换功能替换现有密钥".to_string(),
        ));
    }
    Ok(())
}
/// 吊销 / 续期 / 轮换前的不写库租户与有效密钥归属预检。
///
/// 跨租户前缀或已吊销密钥一律返回 `404 NotFound`，确保处理器在消费 MFA `fresh` 窗口前拦截越权请求。
///
/// # Errors
///
/// 租户不存在、密钥不属于该租户或已吊销、数据库查询失败时返回错误。
pub async fn precheck_tenant_api_key(
    db: &PgPool,
    auth_db: &PgPool,
    id_or_code: &str,
    key_prefix: &str,
) -> Result<uuid::Uuid, ApiError> {
    let tenant_id = crate::services::tenant::resolve_tenant_id(db, id_or_code).await?;
    ensure_tenant_exists(db, tenant_id, id_or_code).await?;
    let exists = repo::active_tenant_api_key_exists(auth_db, tenant_id, key_prefix)
        .await
        .map_err(|error| {
            tracing::error!("failed to check tenant API key ownership: {error}");
            ApiError::internal("检查 API 密钥失败".to_string())
        })?;
    if !exists {
        return Err(ApiError::not_found(
            "api_key".to_string(),
            key_prefix.to_string(),
        ));
    }
    Ok(tenant_id)
}
async fn notify_api_key_cache_invalidation(db: &PgPool) {
    if let Err(error) = mazbri_core::config_watch::bump_config_version_and_notify(
        db,
        mazbri_core::config_watch::AUTH_API_KEY_VERSION_KEY,
    )
    .await
    {
        tracing::warn!(
            error = %error,
            "Failed to bump auth_api_key_version for cross-service cache invalidation"
        );
    }
}
fn compute_rotated_expiration(
    current_expires_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    match current_expires_at {
        None => None,
        Some(exp) if exp > now => Some(exp),
        Some(_) => Some(now + TimeDelta::days(365)),
    }
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_api_key(
    db: &PgPool,
    auth_db: &PgPool,
    audit: &AuditService,
    id_or_code: &str,
    payload: &CreateApiKeyRequest,
    user_id: uuid::Uuid,
    client_ip: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    info!("Creating API key for tenant: {}", id_or_code);
    precheck_create_api_key(db, auth_db, id_or_code, payload.scopes.as_slice()).await?;
    let tenant_id = crate::services::tenant::resolve_tenant_id(db, id_or_code).await?;
    let (full_api_key, prefix, secret_hash, key_hash, key_suffix) = generate_api_key_material()?;
    let expires_at_db = parse_expiration(payload.expires_at, payload.expires_days);
    let mut tx = auth_db
        .begin()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to begin transaction: {e}")))?;
    let credential_id = uuid::Uuid::new_v4();
    let metadata_json = serde_json::json!({ "created_by" : "mgmt-api", });
    let cred_result = repo::insert_credential(
        &mut tx,
        credential_id,
        tenant_id,
        &prefix,
        &secret_hash,
        Some(expires_at_db),
        metadata_json,
    )
    .await;
    if let Err(e) = cred_result {
        let _ = tx.rollback().await;
        return Err(ApiError::internal(format!(
            "Failed to insert credential: {e}"
        )));
    }
    let api_key_id = uuid::Uuid::new_v4();
    let mut scopes_array: Vec<String> = Vec::new();
    for s in payload.scopes.as_slice() {
        let s = s.as_str().to_string();
        if !scopes_array.contains(&s) {
            scopes_array.push(s);
        }
    }
    let key_result = repo::insert_api_key(
        &mut tx,
        repo::InsertApiKeyParams {
            api_key_id,
            credential_id,
            prefix: &prefix,
            key_hash: &key_hash,
            scopes: &scopes_array,
            expires_at: Some(expires_at_db),
            key_suffix: &key_suffix,
        },
    )
    .await;
    let key_info = match key_result {
        Ok(info) => info,
        Err(e) => {
            let _ = tx.rollback().await;
            return Err(ApiError::internal(format!("Failed to insert API key: {e}")));
        }
    };
    tx.commit()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to commit transaction: {e}")))?;
    info!("Created API key {} for tenant {}", prefix, tenant_id);
    audit.log(api_key_audit(
        user_id,
        AuditAction::ApiKeyCreate,
        tenant_id,
        &prefix,
        Some(serde_json::json!({ "scopes" : scopes_array, })),
        client_ip,
    ));
    Ok(serde_json::json!(
        { "api_key" : full_api_key, "key_info" : { "key_prefix" : prefix, "scopes" :
        key_info.scopes, "is_active" : true, "created_at" : key_info.created_at,
        "expires_at" : key_info.expires_at } }
    ))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_api_keys(
    db: &PgPool,
    auth_db: &PgPool,
    id_or_code: &str,
    can_write: bool,
) -> Result<serde_json::Value, ApiError> {
    let tenant_id = crate::services::tenant::resolve_tenant_id(db, id_or_code).await?;
    ensure_tenant_exists(db, tenant_id, id_or_code).await?;
    let keys = repo::list_api_keys(auth_db, tenant_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list API keys: {e}")))?;
    let mapped_keys: Vec<serde_json::Value> = keys
        .iter()
        .map(|k| {
            let suffix = k.key_suffix.as_deref().unwrap_or_else(|| {
                tracing::warn!(
                    prefix = % k.prefix,
                    "API key key_suffix is NULL in database, using '****' for display"
                );
                "****"
            });
            let display_key = mask_api_key(&k.prefix, suffix);
            serde_json::json!(
                { "key_prefix" : k.prefix, "key_suffix" : suffix, "display_key" :
                display_key, "scopes" : k.scopes, "is_active" :
                k.is_active, "created_at" : k.created_at, "expires_at" : k.expires_at }
            )
        })
        .collect();
    let can_create = can_write && mapped_keys.is_empty();
    let can_rotate = can_write && !mapped_keys.is_empty();
    Ok(serde_json::json!(
        { "keys" : mapped_keys, "total" : mapped_keys.len(), "actions" : { "create" :
        can_create, "rotate" : can_rotate, "delete" : can_write } }
    ))
}
pub struct RevokeApiKeyParams<'a> {
    pub db: &'a PgPool,
    pub auth_db: &'a PgPool,
    pub audit: &'a AuditService,
    pub id_or_code: &'a str,
    pub key_prefix: &'a str,
    pub user_id: uuid::Uuid,
    pub username: &'a str,
    pub client_ip: Option<String>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn revoke_api_key(params: RevokeApiKeyParams<'_>) -> Result<serde_json::Value, ApiError> {
    info!(
        "Revoking API key {} for tenant: {} (by {})",
        params.key_prefix, params.id_or_code, params.username
    );
    let tenant_id =
        crate::services::tenant::resolve_tenant_id(params.db, params.id_or_code).await?;
    ensure_tenant_exists(params.db, tenant_id, params.id_or_code).await?;
    let mut tx = params
        .auth_db
        .begin()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to begin transaction: {e}")))?;
    let updated_key =
        match repo::revoke_api_key_by_prefix(&mut tx, tenant_id, params.key_prefix).await {
            Ok(key) => key,
            Err(e) => {
                let _ = tx.rollback().await;
                return Err(ApiError::internal(format!("Failed to revoke API key: {e}")));
            }
        };
    if updated_key.is_none() {
        let _ = tx.rollback().await;
        return Err(ApiError::not_found(
            "api_key".to_string(),
            params.key_prefix.to_string(),
        ));
    }
    if let Err(e) = repo::deactivate_credential_by_prefix(&mut tx, tenant_id, params.key_prefix).await
    {
        let _ = tx.rollback().await;
        return Err(ApiError::internal(format!(
            "Failed to deactivate credential: {e}"
        )));
    }
    tx.commit()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to commit transaction: {e}")))?;
    notify_api_key_cache_invalidation(params.db).await;
    info!(
        "Revoked API key {} for tenant {}",
        params.key_prefix, tenant_id
    );
    params.audit.log(api_key_audit(
        params.user_id,
        AuditAction::ApiKeyRevoke,
        tenant_id,
        params.key_prefix,
        None,
        params.client_ip,
    ));
    Ok(serde_json::json!(
        { "message" : "API key revoked successfully", "key_prefix" : params.key_prefix, }
    ))
}
pub struct UpdateApiKeyParams<'a> {
    pub db: &'a PgPool,
    pub auth_db: &'a PgPool,
    pub audit: &'a AuditService,
    pub id_or_code: &'a str,
    pub key_prefix: &'a str,
    pub payload: &'a UpdateApiKeyRequest,
    pub user_id: uuid::Uuid,
    pub client_ip: Option<String>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_api_key(params: UpdateApiKeyParams<'_>) -> Result<serde_json::Value, ApiError> {
    info!(
        "Updating API key {} for tenant: {}",
        params.key_prefix, params.id_or_code
    );
    let tenant_id =
        crate::services::tenant::resolve_tenant_id(params.db, params.id_or_code).await?;
    ensure_tenant_exists(params.db, tenant_id, params.id_or_code).await?;
    let mut tx = params
        .auth_db
        .begin()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to begin transaction: {e}")))?;
    let current_expires_at =
        match repo::get_api_key_expiration(&mut tx, tenant_id, params.key_prefix).await {
            Ok(Some(exp)) => exp,
            Ok(None) => {
                let _ = tx.rollback().await;
                return Err(ApiError::not_found(
                    "api_key".to_string(),
                    params.key_prefix.to_string(),
                ));
            }
            Err(e) => {
                let _ = tx.rollback().await;
                return Err(ApiError::internal(format!(
                    "Failed to get current expiration: {e}"
                )));
            }
        };
    let now = Utc::now();
    let base_time = current_expires_at.filter(|&exp| exp > now).unwrap_or(now);
    let days_to_add = params.payload.expires_days.unwrap_or(365);
    let new_expires_at = base_time + TimeDelta::days(days_to_add);
    let key_info =
        match repo::update_api_key_expiration(&mut tx, tenant_id, params.key_prefix, new_expires_at)
            .await
        {
            Ok(Some(info)) => info,
            Ok(None) => {
                let _ = tx.rollback().await;
                return Err(ApiError::not_found(
                    "api_key".to_string(),
                    params.key_prefix.to_string(),
                ));
            }
            Err(e) => {
                let _ = tx.rollback().await;
                return Err(ApiError::internal(format!("Failed to update API key: {e}")));
            }
        };
    if let Err(e) =
        repo::update_credential_expiration(&mut tx, tenant_id, params.key_prefix, new_expires_at)
            .await
    {
        let _ = tx.rollback().await;
        return Err(ApiError::internal(format!(
            "Failed to update credential expiration: {e}"
        )));
    }
    tx.commit()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to commit transaction: {e}")))?;
    notify_api_key_cache_invalidation(params.db).await;
    info!(
        "Updated API key {} for tenant {}",
        params.key_prefix, tenant_id
    );
    params.audit.log(api_key_audit(
        params.user_id,
        AuditAction::ApiKeyUpdate,
        tenant_id,
        params.key_prefix,
        Some(serde_json::json!({ "expires_days": days_to_add })),
        params.client_ip,
    ));
    Ok(serde_json::json!(
        { "message" : "API key updated successfully", "key_info" : { "key_prefix" :
        params.key_prefix, "description" : serde_json::Value::Null, "scopes" : key_info
        .scopes, "created_at" : key_info.created_at, "expires_at" : key_info
        .expires_at, } }
    ))
}
pub struct RotateApiKeyParams<'a> {
    pub db: &'a PgPool,
    pub auth_db: &'a PgPool,
    pub audit: &'a AuditService,
    pub id_or_code: &'a str,
    pub key_prefix: &'a str,
    pub user_id: uuid::Uuid,
    pub username: &'a str,
    pub client_ip: Option<String>,
}
struct GeneratedKeyMaterial {
    new_prefix: String,
    secret_hash: String,
    key_hash: String,
    key_suffix: String,
}
async fn rotate_api_key_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: uuid::Uuid,
    old_prefix: &str,
    material: &GeneratedKeyMaterial,
) -> Result<(repo::ApiKeyRow, uuid::Uuid), ApiError> {
    let locked = repo::lock_active_tenant_api_key(tx, tenant_id, old_prefix)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to lock API key for rotation: {e}")))?
        .ok_or_else(|| ApiError::not_found("api_key".to_string(), old_prefix.to_string()))?;
    let revoked = repo::revoke_api_key_by_prefix(tx, tenant_id, old_prefix)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to revoke previous API key: {e}")))?;
    if revoked.is_none() {
        return Err(ApiError::not_found(
            "api_key".to_string(),
            old_prefix.to_string(),
        ));
    }
    repo::deactivate_credential_by_prefix(tx, tenant_id, old_prefix)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to deactivate previous credential: {e}")))?;
    let inherited_expires_at = compute_rotated_expiration(locked.expires_at, Utc::now());
    let credential_id = uuid::Uuid::new_v4();
    let metadata_json = serde_json::json!({
        "created_by": "mgmt-api-rotate",
        "rotated_from_prefix": old_prefix,
        "rotated_from_credential_id": locked.credential_id,
    });
    repo::insert_credential(
        tx,
        credential_id,
        tenant_id,
        &material.new_prefix,
        &material.secret_hash,
        inherited_expires_at,
        metadata_json,
    )
    .await
    .map_err(|e| ApiError::internal(format!("Failed to insert rotated credential: {e}")))?;
    let key_info = repo::insert_api_key(
        tx,
        repo::InsertApiKeyParams {
            api_key_id: uuid::Uuid::new_v4(),
            credential_id,
            prefix: &material.new_prefix,
            key_hash: &material.key_hash,
            scopes: &locked.scopes,
            expires_at: inherited_expires_at,
            key_suffix: &material.key_suffix,
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("Failed to insert rotated API key: {e}")))?;
    Ok((key_info, locked.credential_id))
}

/// 原子轮换租户 API 密钥：单事务软吊销旧密钥并继承 `scopes` 与有效期签发新密钥，提交后广播缓存失效。
///
/// # Errors
///
/// 租户或有效密钥不存在、事务提交或密钥哈希计算失败时返回错误。
pub async fn rotate_api_key(params: RotateApiKeyParams<'_>) -> Result<serde_json::Value, ApiError> {
    info!(
        "Rotating API key {} for tenant: {} (by {})",
        params.key_prefix, params.id_or_code, params.username
    );
    let tenant_id =
        crate::services::tenant::resolve_tenant_id(params.db, params.id_or_code).await?;
    ensure_tenant_exists(params.db, tenant_id, params.id_or_code).await?;
    let (full_api_key, new_prefix, secret_hash, key_hash, key_suffix) = generate_api_key_material()?;
    let material = GeneratedKeyMaterial {
        new_prefix: new_prefix.clone(),
        secret_hash,
        key_hash,
        key_suffix,
    };
    let mut tx = params
        .auth_db
        .begin()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to begin transaction: {e}")))?;
    let (key_info, old_credential_id) =
        match rotate_api_key_in_tx(&mut tx, tenant_id, params.key_prefix, &material).await {
            Ok(res) => res,
            Err(err) => {
                let _ = tx.rollback().await;
                return Err(err);
            }
        };
    tx.commit()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to commit transaction: {e}")))?;
    notify_api_key_cache_invalidation(params.db).await;
    info!(
        "Rotated API key {} -> {} for tenant {}",
        params.key_prefix, new_prefix, tenant_id
    );
    params.audit.log(api_key_audit(
        params.user_id,
        AuditAction::ApiKeyRotate,
        tenant_id,
        &new_prefix,
        Some(serde_json::json!({
            "previous_key_prefix": params.key_prefix,
            "previous_credential_id": old_credential_id,
            "scopes": key_info.scopes,
        })),
        params.client_ip,
    ));
    Ok(serde_json::json!({
        "api_key": full_api_key,
        "previous_key_prefix": params.key_prefix,
        "key_info": {
            "key_prefix": new_prefix,
            "scopes": key_info.scopes,
            "is_active": true,
            "created_at": key_info.created_at,
            "expires_at": key_info.expires_at,
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::{compute_rotated_expiration, generate_api_key_material, mask_api_key};
    use chrono::{TimeDelta, Utc};

    #[test]
    fn rotated_expiration_preserves_none_and_future_timestamps() {
        let now = Utc::now();
        assert_eq!(compute_rotated_expiration(None, now), None);

        let future = now + TimeDelta::days(90);
        assert_eq!(compute_rotated_expiration(Some(future), now), Some(future));

        let expired = now - TimeDelta::days(1);
        assert_eq!(
            compute_rotated_expiration(Some(expired), now),
            Some(now + TimeDelta::days(365))
        );
    }

    #[test]
    fn generated_material_matches_mazbri_key_format() {
        let (full, prefix, secret_hash, key_hash, suffix) =
            generate_api_key_material().expect("key generation");
        assert!(full.starts_with(&format!("mazbri_{prefix}_")));
        assert_eq!(prefix.len(), 16);
        assert!(secret_hash.starts_with("$argon2"));
        assert_eq!(key_hash.len(), 64);
        assert_eq!(suffix.len(), 8);
        assert_eq!(
            mask_api_key(&prefix, &suffix),
            format!("mazbri_{prefix}_****{suffix}")
        );
    }
}
