use crate::error::ApiError;
use crate::repositories::tenant_crypto as repo;
use crate::services::license::{ensure_license_issuance_allowed, fetch_license_status};
use crate::services::root_package::{
    fetch_root_material_bundle, fetch_root_protection_keyring_cached,
};
use crate::services::tenant::resolve_tenant_id;
use mazbri_core::crypto::supplier_secrets::{RootMaterialKeyring, install_root_material_keyring};
use mazbri_core::crypto::tenant_key::{
    COMMERCIAL_ROOT_PROTECTION_WRAPPING_KEY_ID, decode_configured_key, default_key_metadata,
    unwrap_and_verify_tenant_key_with_keyring, wrap_new_tenant_key,
};
use mazbri_core::crypto::{
    AlgorithmId, AlgorithmRegistry, SUITE_AES_256_GCM, TenantTransportProfile, TransportMode,
    resolve_suite,
};
use serde::Deserialize;
use sqlx::PgPool;

const DEFAULT_COMPANION_MAC: &str = "hmac-sha256";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TenantCryptoPurgeResult {
    pub deleted_keys: u64,
    pub reset_profiles: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct UpdateCryptoProfileRequest {
    pub encryption_mode: String,
    #[serde(default)]
    pub suite: Option<String>,
    #[serde(default)]
    pub data_alg: Option<String>,
    #[serde(default)]
    pub mac_alg: Option<String>,
    #[serde(default)]
    pub allow_insecure: bool,
    #[serde(default = "default_require_signature")]
    pub require_signature: bool,
    #[serde(default = "default_nonce_ttl_seconds")]
    pub nonce_ttl_seconds: i32,
    #[serde(default = "default_clock_skew_seconds")]
    pub clock_skew_seconds: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CryptoExportQuery {
    #[serde(default = "default_export_mode")]
    pub mode: String,
}

fn default_export_mode() -> String {
    "regular".to_string()
}

/// 迁移模式导出包含加密后的私钥材料与密钥事件，仅限超级管理员并要求 MFA。
#[must_use]
pub fn is_migration_export(mode: &str) -> bool {
    mode == "migration"
}

fn default_require_signature() -> bool {
    true
}

fn default_nonce_ttl_seconds() -> i32 {
    300
}

fn default_clock_skew_seconds() -> i32 {
    300
}

async fn notify_tenant_crypto_invalidation(pool: &PgPool) {
    if let Err(error) = mazbri_core::config_watch::bump_config_version_and_notify(
        pool,
        mazbri_core::config_watch::AUTH_API_KEY_VERSION_KEY,
    )
    .await
    {
        tracing::warn!(
            error = %error,
            "Failed to bump auth_api_key_version after tenant crypto mutation"
        );
    }
}

async fn ensure_tenant_crypto_enabled(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
) -> Result<(), ApiError> {
    ensure_license_issuance_allowed(client, base_url, token).await
}

async fn load_root_materials(
    cert_client: &reqwest::Client,
    cert_service_url: &str,
    cert_service_token: &str,
) -> Result<(Vec<u8>, Vec<u8>), ApiError> {
    let bundle =
        fetch_root_material_bundle(cert_client, cert_service_url, cert_service_token).await?;
    let master_key = decode_configured_key(&bundle.master_key)
        .map_err(|error| ApiError::internal(format!("Tenant crypto master key error: {error}")))?;
    let integrity_key = decode_configured_key(&bundle.integrity_key).map_err(|error| {
        ApiError::internal(format!("Tenant crypto integrity key error: {error}"))
    })?;
    Ok((master_key, integrity_key))
}

fn parse_optional_alg(raw: Option<&str>, field: &str) -> Result<Option<AlgorithmId>, ApiError> {
    match raw {
        None | Some("") => Ok(None),
        Some(value) => value.parse::<AlgorithmId>().map(Some).map_err(|_| {
            ApiError::validation(format!(
                "unsupported {field}: {value} (kebab-case closed set only)"
            ))
        }),
    }
}

fn parse_transport_mode(raw: &str) -> Result<TransportMode, ApiError> {
    match raw {
        "none" => Ok(TransportMode::None),
        "request" => Ok(TransportMode::Request),
        "full" => Ok(TransportMode::Full),
        other => Err(ApiError::validation(format!(
            "invalid encryption_mode: {other}"
        ))),
    }
}

fn map_crypto_validation(error: &mazbri_core::crypto::CryptoError) -> ApiError {
    ApiError::validation(error.to_string())
}

fn normalized_optional_string(value: Option<&String>) -> Option<&str> {
    value
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

impl UpdateCryptoProfileRequest {
    fn validate_against_profile(&self) -> Result<TenantTransportProfile, ApiError> {
        if !(60..=3600).contains(&self.nonce_ttl_seconds) {
            return Err(ApiError::validation(
                "nonce_ttl_seconds must be between 60 and 3600",
            ));
        }
        if !(60..=3600).contains(&self.clock_skew_seconds) {
            return Err(ApiError::validation(
                "clock_skew_seconds must be between 60 and 3600",
            ));
        }

        let mode = parse_transport_mode(&self.encryption_mode)?;
        let suite = normalized_optional_string(self.suite.as_ref()).map(str::to_owned);
        let data_alg =
            parse_optional_alg(normalized_optional_string(self.data_alg.as_ref()), "data_alg")?;
        let mac_alg =
            parse_optional_alg(normalized_optional_string(self.mac_alg.as_ref()), "mac_alg")?;

        let (suite, data_alg, mac_alg, allow_insecure, require_signature) =
            if matches!(mode, TransportMode::None) {
                (None, None, None, false, false)
            } else {
                (
                    suite,
                    data_alg,
                    mac_alg,
                    self.allow_insecure,
                    self.require_signature,
                )
            };

        let profile = TenantTransportProfile {
            mode,
            suite,
            data_alg,
            mac_alg,
            require_signature,
            allow_insecure,
            nonce_ttl_secs: u32::try_from(self.nonce_ttl_seconds).map_err(|_| {
                ApiError::validation("nonce_ttl_seconds must be between 60 and 3600")
            })?,
            clock_skew_secs: u32::try_from(self.clock_skew_seconds).map_err(|_| {
                ApiError::validation("clock_skew_seconds must be between 60 and 3600")
            })?,
        };
        profile
            .validate(AlgorithmRegistry::global())
            .map_err(|error| map_crypto_validation(&error))?;
        Ok(profile)
    }
}

fn persistence_fields(
    validated: &TenantTransportProfile,
) -> (&str, Option<&str>, Option<&str>, Option<&str>, bool, bool) {
    let mode = match validated.mode {
        TransportMode::None => "none",
        TransportMode::Request => "request",
        TransportMode::Full => "full",
    };
    (
        mode,
        validated.suite.as_deref(),
        validated.data_alg.map(AlgorithmId::as_str),
        validated.mac_alg.map(AlgorithmId::as_str),
        validated.require_signature,
        validated.allow_insecure,
    )
}

fn resolve_key_algorithms(
    profile: &repo::TenantCryptoProfileRow,
) -> Result<(String, String), ApiError> {
    if profile.encryption_mode == "none" {
        let spec = resolve_suite(SUITE_AES_256_GCM).map_err(|error| map_crypto_validation(&error))?;
        let mac = spec
            .mac_alg
            .map_or(DEFAULT_COMPANION_MAC, AlgorithmId::as_str);
        return Ok((spec.data_alg.as_str().to_owned(), mac.to_owned()));
    }

    let mode = parse_transport_mode(&profile.encryption_mode)?;
    let data_alg = parse_optional_alg(profile.data_alg.as_deref(), "data_alg")?;
    let mac_alg = parse_optional_alg(profile.mac_alg.as_deref(), "mac_alg")?;
    let transport = TenantTransportProfile {
        mode,
        suite: profile.suite.clone(),
        data_alg,
        mac_alg,
        require_signature: profile.require_signature,
        allow_insecure: profile.allow_insecure,
        nonce_ttl_secs: u32::try_from(profile.nonce_ttl_seconds).unwrap_or(300),
        clock_skew_secs: u32::try_from(profile.clock_skew_seconds).unwrap_or(300),
    };
    let (data, mac) = transport
        .effective_algs()
        .map_err(|error| map_crypto_validation(&error))?;
    let data = data
        .ok_or_else(|| {
            ApiError::validation("profile requires data_alg or suite before creating keys")
        })?
        .as_str()
        .to_owned();
    let mac = mac.map_or(DEFAULT_COMPANION_MAC.to_owned(), |id| id.as_str().to_owned());
    Ok((data, mac))
}

fn key_metadata_for(data_alg: &str, mac_alg: &str, suite: Option<&str>) -> serde_json::Value {
    let mut meta = default_key_metadata();
    if let Some(object) = meta.as_object_mut() {
        object.insert(
            "dataAlg".to_owned(),
            serde_json::Value::String(data_alg.to_owned()),
        );
        object.insert(
            "macAlg".to_owned(),
            serde_json::Value::String(mac_alg.to_owned()),
        );
        if let Some(suite_name) = suite {
            object.insert(
                "suite".to_owned(),
                serde_json::Value::String(suite_name.to_owned()),
            );
        }
    }
    meta
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_profile(pool: &PgPool, code: &str) -> Result<serde_json::Value, ApiError> {
    let tenant_id = resolve_tenant_id(pool, code).await?;
    let profile = repo::upsert_profile_defaults(pool, tenant_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to get crypto profile: {error}")))?;
    Ok(serde_json::to_value(profile).unwrap_or_default())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_profile(
    pool: &PgPool,
    cert_client: &reqwest::Client,
    cert_service_url: &str,
    cert_service_token: &str,
    code: &str,
    payload: &UpdateCryptoProfileRequest,
    actor_id: uuid::Uuid,
) -> Result<serde_json::Value, ApiError> {
    let validated = payload.validate_against_profile()?;
    let tenant_id = resolve_tenant_id(pool, code).await?;
    if !matches!(validated.mode, TransportMode::None) {
        ensure_tenant_crypto_enabled(cert_client, cert_service_url, cert_service_token).await?;
        let key = repo::find_key(pool, tenant_id)
            .await
            .map_err(|error| ApiError::internal(format!("Failed to check crypto key: {error}")))?;
        if key.is_none() {
            return Err(ApiError::validation(
                "启用传输加密前请先生成租户加密密钥",
            ));
        }
    }

    let (mode, suite, data_alg, mac_alg, require_signature, allow_insecure) =
        persistence_fields(&validated);
    let profile = repo::update_profile(
        pool,
        repo::UpdateCryptoProfileParams {
            tenant_id,
            encryption_mode: mode,
            suite,
            data_alg,
            mac_alg,
            require_signature,
            allow_insecure,
            nonce_ttl_seconds: payload.nonce_ttl_seconds,
            clock_skew_seconds: payload.clock_skew_seconds,
            actor_id,
        },
    )
    .await
    .map_err(|error| ApiError::internal(format!("Failed to update crypto profile: {error}")))?;
    notify_tenant_crypto_invalidation(pool).await;
    Ok(serde_json::to_value(profile).unwrap_or_default())
}

/// 租户传输加密页单一读取契约：`profile` + `key` + `license_status` + `actions`。
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_crypto_bundle(
    pool: &PgPool,
    cert_client: &reqwest::Client,
    cert_service_url: &str,
    cert_service_token: &str,
    code: &str,
    can_write: bool,
    is_super_admin: bool,
) -> Result<serde_json::Value, ApiError> {
    let tenant_id = resolve_tenant_id(pool, code).await?;
    let profile = repo::upsert_profile_defaults(pool, tenant_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to get crypto profile: {error}")))?;
    let key = repo::find_key(pool, tenant_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to get crypto key: {error}")))?;
    let license_status =
        fetch_license_status(cert_client, cert_service_url, cert_service_token).await?;
    Ok(serde_json::json!({
        "profile": profile,
        "key": key,
        "license_status": license_status,
        "actions": {
            "update_profile": can_write,
            "create_key": can_write,
            "rotate_key": can_write,
            "delete_key": can_write,
            "allow_insecure": is_super_admin,
            "manage_root_package": is_super_admin,
        }
    }))
}

async fn log_create_key_events_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: uuid::Uuid,
    key_id: uuid::Uuid,
    rotate: bool,
    actor_id: uuid::Uuid,
    details: &serde_json::Value,
) -> Result<(), ApiError> {
    repo::log_key_event_tx(
        tx,
        tenant_id,
        Some(key_id),
        if rotate { "rotate" } else { "create" },
        actor_id,
        details,
    )
    .await
    .map_err(|error| ApiError::internal(format!("Failed to log crypto key event: {error}")))?;
    repo::log_key_event_tx(
        tx,
        tenant_id,
        Some(key_id),
        "reveal_once",
        actor_id,
        &serde_json::json!({ "shownOnce": true }),
    )
    .await
    .map_err(|error| ApiError::internal(format!("Failed to log crypto key event: {error}")))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_key(
    pool: &PgPool,
    cert_client: &reqwest::Client,
    cert_service_url: &str,
    cert_service_token: &str,
    code: &str,
    actor_id: uuid::Uuid,
    rotate: bool,
) -> Result<serde_json::Value, ApiError> {
    ensure_tenant_crypto_enabled(cert_client, cert_service_url, cert_service_token).await?;
    let tenant_id = resolve_tenant_id(pool, code).await?;
    let (master_key, integrity_key) =
        load_root_materials(cert_client, cert_service_url, cert_service_token).await?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|error| ApiError::internal(format!("Failed to start transaction: {error}")))?;
    if repo::lock_tenant_tx(&mut tx, tenant_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to lock tenant: {error}")))?
        .is_none()
    {
        return Err(ApiError::not_found("tenant".to_string(), code.to_string()));
    }

    let existing = repo::find_key_tx(&mut tx, tenant_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to check crypto key: {error}")))?;
    if !rotate && existing.is_some() {
        return Err(ApiError::validation(
            "租户已存在传输加密密钥，请使用轮换密钥",
        ));
    }

    let profile = repo::upsert_profile_defaults_tx(&mut tx, tenant_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to load crypto profile: {error}")))?;
    let (data_algorithm, signature_algorithm) = resolve_key_algorithms(&profile)?;
    let wrapped = wrap_new_tenant_key(
        &tenant_id.to_string(),
        &master_key,
        &integrity_key,
        COMMERCIAL_ROOT_PROTECTION_WRAPPING_KEY_ID,
    )
    .map_err(|error| ApiError::internal(format!("Failed to wrap tenant key: {error}")))?;
    let key_metadata = key_metadata_for(
        &data_algorithm,
        &signature_algorithm,
        profile.suite.as_deref(),
    );
    let key = repo::upsert_key_tx(
        &mut tx,
        repo::UpsertCryptoKeyParams {
            tenant_id,
            encrypted_secret_key: &wrapped.encrypted_secret_key,
            secret_key_digest: &wrapped.secret_key_digest,
            wrapping_key_id: &wrapped.wrapping_key_id,
            actor_id,
            metadata: &key_metadata,
        },
    )
    .await
    .map_err(|error| ApiError::internal(format!("Failed to save crypto key: {error}")))?;
    log_create_key_events_tx(
        &mut tx,
        tenant_id,
        key.id,
        rotate,
        actor_id,
        &serde_json::json!({
            "dataAlg": data_algorithm,
            "macAlg": signature_algorithm
        }),
    )
    .await?;
    tx.commit()
        .await
        .map_err(|error| ApiError::internal(format!("Failed to commit transaction: {error}")))?;
    notify_tenant_crypto_invalidation(pool).await;
    Ok(serde_json::json!({
        "id": key.id,
        "tenantSecretKey": wrapped.tenant_secret_key_b64,
        "shownOnce": true,
        "createdAt": key.created_at,
        "updatedAt": key.updated_at
    }))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn export_crypto_data(
    pool: &PgPool,
    query: &CryptoExportQuery,
    actor_id: uuid::Uuid,
) -> Result<serde_json::Value, ApiError> {
    let include_encrypted_secret_key = match query.mode.as_str() {
        "regular" => false,
        "migration" => true,
        _ => return Err(ApiError::validation("mode must be regular or migration")),
    };
    let profiles = repo::list_all_profiles(pool).await.map_err(|error| {
        ApiError::internal(format!("Failed to export crypto profiles: {error}"))
    })?;
    let keys = repo::list_all_keys(pool)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to export crypto keys: {error}")))?;
    let events = if include_encrypted_secret_key {
        repo::list_all_key_events(pool).await.map_err(|error| {
            ApiError::internal(format!("Failed to export crypto key events: {error}"))
        })?
    } else {
        Vec::new()
    };
    let exported_keys = keys
        .iter()
        .map(|key| {
            let mut value = serde_json::to_value(key).unwrap_or_default();
            if let Some(object) = value.as_object_mut() {
                if include_encrypted_secret_key {
                    object.insert(
                        "encrypted_secret_key".to_string(),
                        serde_json::Value::String(key.encrypted_secret_key.clone()),
                    );
                } else {
                    object.remove("encrypted_secret_key");
                }
            }
            value
        })
        .collect::<Vec<_>>();
    for tenant_id in profiles.iter().map(|profile| profile.tenant_id) {
        let _ = repo::log_key_event(
            pool,
            tenant_id,
            None,
            "export",
            actor_id,
            &serde_json::json!({ "mode": query.mode }),
        )
        .await;
    }
    let wrapping_key_ids = keys
        .iter()
        .map(|key| key.wrapping_key_id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "manifest": {
            "format": "mazbri-tenant-export-v1",
            "exportedAt": chrono::Utc::now(),
            "mode": query.mode,
            "tenantCount": profiles.len(),
            "cryptoKeyCount": keys.len(),
            "wrappingKeyIds": wrapping_key_ids,
            "tables": {
                "tenant_crypto_profiles": { "rowCount": profiles.len() },
                "tenant_crypto_keys": { "rowCount": keys.len() },
                "tenant_crypto_key_events": { "rowCount": events.len() }
            }
        },
        "tables": {
            "tenant_crypto_profiles": profiles,
            "tenant_crypto_keys": exported_keys,
            "tenant_crypto_key_events": events
        }
    }))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn verify_crypto_keys(
    pool: &PgPool,
    cert_client: &reqwest::Client,
    cert_service_url: &str,
    cert_service_token: &str,
    actor_id: uuid::Uuid,
) -> Result<serde_json::Value, ApiError> {
    let protection =
        fetch_root_protection_keyring_cached(cert_client, cert_service_url, cert_service_token)
            .await?;
    let keyring_val = RootMaterialKeyring::from_protection_keyring(&protection)
        .map_err(|error| ApiError::internal(format!("Root material keyring error: {error}")))?;
    let keyring = install_root_material_keyring(keyring_val);
    let keys = repo::list_all_keys(pool)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to list crypto keys: {error}")))?;
    let mut verified = 0usize;
    let mut failed = Vec::new();
    for key in &keys {
        let tenant_id = key.tenant_id.to_string();
        let result = unwrap_and_verify_tenant_key_with_keyring(
            &key.encrypted_secret_key,
            &tenant_id,
            &key.secret_key_digest,
            &keyring,
        );
        match result {
            Ok(_) => {
                verified += 1;
                let _ = repo::log_key_event(
                    pool,
                    key.tenant_id,
                    Some(key.id),
                    "verify",
                    actor_id,
                    &serde_json::json!({ "result": "ok" }),
                )
                .await;
            }
            Err(_) => failed.push(serde_json::json!({
                "tenant_id": key.tenant_id,
                "key_id": key.id,
            })),
        }
    }
    Ok(serde_json::json!({
        "checked": keys.len(),
        "verified": verified,
        "failed": failed
    }))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn purge_all_crypto_materials(
    pool: &PgPool,
    actor_id: uuid::Uuid,
) -> Result<TenantCryptoPurgeResult, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| ApiError::internal(format!("Failed to start crypto purge: {error}")))?;
    let deleted_keys = sqlx::query("DELETE FROM tenant_crypto_keys")
        .execute(&mut *tx)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to delete crypto keys: {error}")))?
        .rows_affected();
    let reset_profiles = sqlx::query(
        r"
        UPDATE tenant_crypto_profiles
        SET encryption_mode = 'none',
            suite = NULL,
            data_alg = NULL,
            mac_alg = NULL,
            allow_insecure = false,
            require_signature = false,
            updated_by = $1,
            updated_at = NOW()
        WHERE encryption_mode <> 'none'
           OR suite IS NOT NULL
           OR data_alg IS NOT NULL
           OR mac_alg IS NOT NULL
           OR allow_insecure
           OR require_signature
        ",
    )
    .bind(actor_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        ApiError::internal(format!("Failed to reset tenant crypto profiles: {error}"))
    })?
    .rows_affected();
    tx.commit()
        .await
        .map_err(|error| ApiError::internal(format!("Failed to commit crypto purge: {error}")))?;
    Ok(TenantCryptoPurgeResult {
        deleted_keys,
        reset_profiles,
    })
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_crypto_key(
    pool: &PgPool,
    code: &str,
    actor_id: uuid::Uuid,
) -> Result<serde_json::Value, ApiError> {
    let tenant_id = resolve_tenant_id(pool, code).await?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|error| ApiError::internal(format!("Failed to start transaction: {error}")))?;

    let key = repo::delete_crypto_key_tx(&mut tx, tenant_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to delete crypto key: {error}")))?
        .ok_or_else(|| ApiError::not_found("tenant_crypto_key".to_string(), code.to_string()))?;

    sqlx::query(
        r"
        UPDATE tenant_crypto_profiles
        SET encryption_mode = 'none',
            suite = NULL,
            data_alg = NULL,
            mac_alg = NULL,
            allow_insecure = false,
            require_signature = false,
            updated_by = $1,
            updated_at = NOW()
        WHERE tenant_id = $2
        ",
    )
    .bind(actor_id)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        ApiError::internal(format!("Failed to reset tenant crypto profile: {error}"))
    })?;

    repo::log_key_event_tx(
        &mut tx,
        tenant_id,
        None,
        "delete",
        actor_id,
        &serde_json::json!({ "status": "deleted" }),
    )
    .await
    .map_err(|error| ApiError::internal(format!("Failed to log crypto key event: {error}")))?;

    tx.commit()
        .await
        .map_err(|error| ApiError::internal(format!("Failed to commit transaction: {error}")))?;

    notify_tenant_crypto_invalidation(pool).await;
    Ok(serde_json::to_value(key).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> UpdateCryptoProfileRequest {
        UpdateCryptoProfileRequest {
            encryption_mode: "full".into(),
            suite: Some(SUITE_AES_256_GCM.into()),
            data_alg: None,
            mac_alg: None,
            allow_insecure: false,
            require_signature: false,
            nonce_ttl_seconds: 300,
            clock_skew_seconds: 300,
        }
    }

    #[test]
    fn recommended_suite_profile_validates() {
        let req = base_request();
        let profile = req.validate_against_profile().expect("valid");
        assert_eq!(profile.mode, TransportMode::Full);
        assert_eq!(profile.suite.as_deref(), Some(SUITE_AES_256_GCM));
    }

    #[test]
    fn legacy_without_allow_insecure_is_rejected() {
        let mut req = base_request();
        req.suite = None;
        req.data_alg = Some("aes-256-ecb".into());
        req.mac_alg = Some("hmac-sha256".into());
        req.allow_insecure = false;
        assert!(req.validate_against_profile().is_err());
    }

    #[test]
    fn legacy_with_allow_insecure_is_accepted() {
        let mut req = base_request();
        req.suite = None;
        req.data_alg = Some("aes-256-ecb".into());
        req.mac_alg = Some("hmac-sha256".into());
        req.allow_insecure = true;
        req.validate_against_profile().expect("legacy allowed");
    }

    #[test]
    fn mode_none_clears_algorithms() {
        let mut req = base_request();
        req.encryption_mode = "none".into();
        req.require_signature = true;
        req.suite = Some(SUITE_AES_256_GCM.into());
        let profile = req.validate_against_profile().expect("none ok");
        assert!(profile.suite.is_none());
        assert!(!profile.require_signature);
        assert!(!profile.allow_insecure);
    }

    #[test]
    fn uppercase_algorithm_is_rejected() {
        let mut req = base_request();
        req.suite = None;
        req.data_alg = Some("AES-256-GCM".into());
        assert!(req.validate_against_profile().is_err());
    }
}
