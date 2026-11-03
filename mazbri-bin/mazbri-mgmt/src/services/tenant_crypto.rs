use crate::error::ApiError;
use crate::repositories::tenant_crypto as repo;
use crate::services::license::{ensure_license_issuance_allowed, fetch_license_status};
use crate::services::root_package::fetch_root_material_bundle;
use crate::services::tenant::resolve_tenant_id;
use mazbri_core::crypto::tenant_key::{
    COMMERCIAL_ROOT_PROTECTION_WRAPPING_KEY_ID, decode_configured_key, default_key_metadata,
    generate_key_version, wrap_new_tenant_key,
};
use mazbri_core::crypto::tenant_key::{unwrap_tenant_key, verify_secret_key_digest};
use mazbri_core::crypto::{
    AlgorithmId, AlgorithmRegistry, BindingSpec, KeySelection, TenantTransportProfile,
    TransportMode, SUITE_TENANT_DEFAULT_2026_11, resolve_suite,
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
    #[serde(default)]
    pub require_request_binding: bool,
    #[serde(default = "default_signature_version")]
    pub signature_version: String,
    #[serde(default = "default_nonce_ttl_seconds")]
    pub nonce_ttl_seconds: i32,
    #[serde(default = "default_clock_skew_seconds")]
    pub clock_skew_seconds: i32,
    #[serde(default)]
    pub current_key_version: Option<String>,
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
fn default_signature_version() -> String {
    "hmac-v1".to_string()
}

fn default_nonce_ttl_seconds() -> i32 {
    300
}

fn default_clock_skew_seconds() -> i32 {
    300
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
        if !matches!(self.signature_version.as_str(), "hmac-v1" | "rsa-v1") {
            return Err(ApiError::validation("invalid signature_version"));
        }
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
        let data_alg = parse_optional_alg(normalized_optional_string(self.data_alg.as_ref()), "data_alg")?;
        let mac_alg = parse_optional_alg(normalized_optional_string(self.mac_alg.as_ref()), "mac_alg")?;

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

        let key = match mode {
            TransportMode::None => None,
            TransportMode::Request | TransportMode::Full => {
                let version = self
                    .current_key_version
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        ApiError::validation(
                            "current_key_version is required when encryption is enabled",
                        )
                    })?;
                Some(KeySelection::Latest {
                    key_id: version.to_owned(),
                })
            }
        };

        let profile = TenantTransportProfile {
            version: 1,
            mode,
            suite,
            data_alg,
            mac_alg,
            key,
            require_signature,
            allow_insecure,
            binding: self
                .require_request_binding
                .then_some(BindingSpec { required: true }),
            nonce_ttl_secs: u32::try_from(self.nonce_ttl_seconds).map_err(|_| {
                ApiError::validation("nonce_ttl_seconds must be between 60 and 3600")
            })?,
            clock_skew_secs: u32::try_from(self.clock_skew_seconds).map_err(|_| {
                ApiError::validation("clock_skew_seconds must be between 60 and 3600")
            })?,
        };
        profile
            .validate(&AlgorithmRegistry::default_registry())
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
        let spec = resolve_suite(SUITE_TENANT_DEFAULT_2026_11).map_err(|error| map_crypto_validation(&error))?;
        let mac = spec
            .mac_alg
            .map_or(DEFAULT_COMPANION_MAC, AlgorithmId::as_str);
        return Ok((spec.data_alg.as_str().to_owned(), mac.to_owned()));
    }

    let mode = parse_transport_mode(&profile.encryption_mode)?;
    let data_alg = parse_optional_alg(profile.data_alg.as_deref(), "data_alg")?;
    let mac_alg = parse_optional_alg(profile.mac_alg.as_deref(), "mac_alg")?;
    let transport = TenantTransportProfile {
        version: u32::try_from(profile.profile_version).unwrap_or(1).max(1),
        mode,
        suite: profile.suite.clone(),
        data_alg,
        mac_alg,
        key: Some(KeySelection::Latest {
            key_id: profile
                .current_key_version
                .clone()
                .unwrap_or_else(|| "pending".to_owned()),
        }),
        require_signature: profile.require_signature,
        allow_insecure: profile.allow_insecure,
        binding: None,
        nonce_ttl_secs: u32::try_from(profile.nonce_ttl_seconds).unwrap_or(300),
        clock_skew_secs: u32::try_from(profile.clock_skew_seconds).unwrap_or(300),
    };
    let (data, mac) = transport.effective_algs().map_err(|error| map_crypto_validation(&error))?;
    let data = data
        .ok_or_else(|| ApiError::validation("profile requires data_alg or suite before creating keys"))?
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
        let version = payload
            .current_key_version
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ApiError::validation("current_key_version is required when encryption is enabled")
            })?;
        let key = repo::find_key(pool, tenant_id, version)
            .await
            .map_err(|error| ApiError::internal(format!("Failed to check crypto key: {error}")))?;
        if !matches!(key.as_ref().map(|row| row.status.as_str()), Some("active")) {
            return Err(ApiError::validation(
                "current_key_version must reference an active key",
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
            require_request_binding: payload.require_request_binding,
            signature_version: &payload.signature_version,
            nonce_ttl_seconds: payload.nonce_ttl_seconds,
            clock_skew_seconds: payload.clock_skew_seconds,
            current_key_version: if matches!(validated.mode, TransportMode::None) {
                None
            } else {
                payload.current_key_version.as_deref()
            },
            actor_id,
        },
    )
    .await
    .map_err(|error| ApiError::internal(format!("Failed to update crypto profile: {error}")))?;
    Ok(serde_json::to_value(profile).unwrap_or_default())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_keys(pool: &PgPool, code: &str) -> Result<serde_json::Value, ApiError> {
    let tenant_id = resolve_tenant_id(pool, code).await?;
    let keys = repo::list_keys(pool, tenant_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to list crypto keys: {error}")))?;
    Ok(serde_json::json!({ "keys": keys }))
}

/// 租户传输加密页单一读取契约：`profile` + `keys` + `license_status` + `actions`。
///
/// `license_status` 经 cert 服务令牌拉取，**不按角色裁剪**；写能力只体现在 `actions`。
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
    let profile = get_profile(pool, code).await?;
    let keys_value = list_keys(pool, code).await?;
    let keys = keys_value
        .get("keys")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    let license_status =
        fetch_license_status(cert_client, cert_service_url, cert_service_token).await?;
    Ok(serde_json::json!({
        "profile": profile,
        "keys": keys,
        "license_status": license_status,
        "actions": {
            "update_profile": can_write,
            "create_key": can_write,
            "rotate_key": can_write,
            "activate_key": can_write,
            "deprecate_key": can_write,
            "disable_key": can_write,
            "delete_key": can_write,
            "allow_insecure": is_super_admin,
            "manage_root_package": is_super_admin,
        }
    }))
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
    let profile = repo::upsert_profile_defaults(pool, tenant_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to load crypto profile: {error}")))?;
    let (data_algorithm, signature_algorithm) = resolve_key_algorithms(&profile)?;
    let existing = repo::list_keys(pool, tenant_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to list crypto keys: {error}")))?;
    let versions = existing
        .iter()
        .map(|row| row.key_version.clone())
        .collect::<Vec<_>>();
    let key_version = generate_key_version(&versions);
    let (master_key, integrity_key) =
        load_root_materials(cert_client, cert_service_url, cert_service_token).await?;
    let wrapped = wrap_new_tenant_key(
        &tenant_id.to_string(),
        &key_version,
        &master_key,
        &integrity_key,
        COMMERCIAL_ROOT_PROTECTION_WRAPPING_KEY_ID,
    )
    .map_err(|error| ApiError::internal(format!("Failed to wrap tenant key: {error}")))?;
    let active_count = repo::active_key_count(pool, tenant_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to count active keys: {error}")))?;
    let status = if rotate || active_count > 0 {
        "pending"
    } else {
        "active"
    };
    let key_metadata = key_metadata_for(
        &data_algorithm,
        &signature_algorithm,
        profile.suite.as_deref(),
    );
    let key = repo::insert_key(
        pool,
        repo::InsertCryptoKeyParams {
            tenant_id,
            key_version: &key_version,
            encrypted_secret_key: &wrapped.encrypted_secret_key,
            secret_key_digest: &wrapped.secret_key_digest,
            wrapping_key_id: &wrapped.wrapping_key_id,
            data_algorithm: &data_algorithm,
            signature_algorithm: &signature_algorithm,
            status,
            actor_id,
            metadata: &key_metadata,
        },
    )
    .await
    .map_err(|error| ApiError::internal(format!("Failed to create crypto key: {error}")))?;
    if status == "active" {
        repo::set_current_key_version(pool, tenant_id, Some(&key_version), actor_id)
            .await
            .map_err(|error| {
                ApiError::internal(format!("Failed to update current crypto key: {error}"))
            })?;
    }
    repo::log_key_event(
        pool,
        tenant_id,
        Some(key.id),
        Some(&key_version),
        if rotate { "rotate" } else { "create" },
        actor_id,
        &serde_json::json!({
            "status": status,
            "dataAlg": data_algorithm,
            "macAlg": signature_algorithm
        }),
    )
    .await
    .map_err(|error| ApiError::internal(format!("Failed to log crypto key event: {error}")))?;
    repo::log_key_event(
        pool,
        tenant_id,
        Some(key.id),
        Some(&key_version),
        "reveal_once",
        actor_id,
        &serde_json::json!({ "shownOnce": true }),
    )
    .await
    .map_err(|error| ApiError::internal(format!("Failed to log crypto key event: {error}")))?;
    Ok(serde_json::json!({
        "keyVersion": key_version,
        "tenantSecretKey": wrapped.tenant_secret_key_b64,
        "algorithm": data_algorithm,
        "signatureAlgorithm": signature_algorithm,
        "shownOnce": true,
        "status": status
    }))
}

pub struct SetKeyStatusArgs<'a> {
    pub pool: &'a PgPool,
    pub cert_client: &'a reqwest::Client,
    pub cert_service_url: &'a str,
    pub cert_service_token: &'a str,
    pub code: &'a str,
    pub version: &'a str,
    pub status: &'a str,
    pub actor_id: uuid::Uuid,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn set_key_status(args: SetKeyStatusArgs<'_>) -> Result<serde_json::Value, ApiError> {
    let pool = args.pool;
    let cert_client = args.cert_client;
    let cert_service_url = args.cert_service_url;
    let cert_service_token = args.cert_service_token;
    let code = args.code;
    let version = args.version;
    let status = args.status;
    let actor_id = args.actor_id;

    if !matches!(status, "active" | "deprecated" | "disabled") {
        return Err(ApiError::validation("invalid key status transition"));
    }
    if matches!(status, "active" | "deprecated") {
        ensure_tenant_crypto_enabled(cert_client, cert_service_url, cert_service_token).await?;
    }
    let tenant_id = resolve_tenant_id(pool, code).await?;
    if status == "active" {
        repo::deprecate_other_active_keys(pool, tenant_id, version, actor_id)
            .await
            .map_err(|error| {
                ApiError::internal(format!("Failed to deprecate old crypto keys: {error}"))
            })?;
    }
    let key = repo::update_key_status(pool, tenant_id, version, status, actor_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to update crypto key: {error}")))?
        .ok_or_else(|| ApiError::not_found("tenant_crypto_key".to_string(), version.to_string()))?;
    if status == "active" {
        repo::set_current_key_version(pool, tenant_id, Some(version), actor_id)
            .await
            .map_err(|error| {
                ApiError::internal(format!("Failed to update current crypto key: {error}"))
            })?;
    }
    repo::log_key_event(
        pool,
        tenant_id,
        Some(key.id),
        Some(version),
        match status {
            "active" => "activate",
            "deprecated" => "deprecate",
            "disabled" => "disable",
            _ => unreachable!(),
        },
        actor_id,
        &serde_json::json!({ "status": status }),
    )
    .await
    .map_err(|error| ApiError::internal(format!("Failed to log crypto key event: {error}")))?;
    Ok(serde_json::to_value(key).unwrap_or_default())
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
    let (master_key, integrity_key) =
        load_root_materials(cert_client, cert_service_url, cert_service_token).await?;
    let keys = repo::list_all_keys(pool)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to list crypto keys: {error}")))?;
    let mut verified = 0usize;
    let mut failed = Vec::new();
    for key in &keys {
        let tenant_id = key.tenant_id.to_string();
        let result = unwrap_tenant_key(
            &key.encrypted_secret_key,
            &tenant_id,
            &key.key_version,
            &master_key,
        )
        .and_then(|secret| {
            verify_secret_key_digest(
                &integrity_key,
                &tenant_id,
                &key.key_version,
                &secret,
                &key.secret_key_digest,
            )
        });
        match result {
            Ok(true) => {
                verified += 1;
                let _ = repo::log_key_event(
                    pool,
                    key.tenant_id,
                    Some(key.id),
                    Some(&key.key_version),
                    "verify",
                    actor_id,
                    &serde_json::json!({ "result": "ok" }),
                )
                .await;
            }
            _ => failed.push(serde_json::json!({
                "tenant_id": key.tenant_id,
                "key_version": key.key_version,
                "status": key.status
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
    sqlx::query("DELETE FROM tenant_request_keys")
        .execute(&mut *tx)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to delete request keys: {error}")))?;
    let reset_profiles = sqlx::query(
        r"
        UPDATE tenant_crypto_profiles
        SET encryption_mode = 'none',
            suite = NULL,
            data_alg = NULL,
            mac_alg = NULL,
            allow_insecure = false,
            require_signature = false,
            current_key_version = NULL,
            updated_by = $1,
            updated_at = NOW()
        WHERE encryption_mode <> 'none'
           OR current_key_version IS NOT NULL
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
    version: &str,
    actor_id: uuid::Uuid,
) -> Result<serde_json::Value, ApiError> {
    let tenant_id = resolve_tenant_id(pool, code).await?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|error| ApiError::internal(format!("Failed to start transaction: {error}")))?;

    let key = repo::delete_crypto_key_tx(&mut tx, tenant_id, version)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to delete crypto key: {error}")))?
        .ok_or_else(|| ApiError::not_found("tenant_crypto_key".to_string(), version.to_string()))?;

    sqlx::query(
        r"
        UPDATE tenant_crypto_profiles
        SET current_key_version = NULL, updated_by = $1, updated_at = NOW()
        WHERE tenant_id = $2 AND current_key_version = $3
        ",
    )
    .bind(actor_id)
    .bind(tenant_id)
    .bind(version)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        ApiError::internal(format!("Failed to update tenant crypto profile: {error}"))
    })?;

    repo::log_key_event_tx(
        &mut tx,
        tenant_id,
        None,
        Some(version),
        "delete",
        actor_id,
        &serde_json::json!({ "status": "deleted" }),
    )
    .await
    .map_err(|error| ApiError::internal(format!("Failed to log crypto key event: {error}")))?;

    tx.commit()
        .await
        .map_err(|error| ApiError::internal(format!("Failed to commit transaction: {error}")))?;

    Ok(serde_json::to_value(key).unwrap_or_default())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_request_keys(pool: &PgPool, code: &str) -> Result<serde_json::Value, ApiError> {
    let tenant_id = resolve_tenant_id(pool, code).await?;
    let keys = repo::list_request_keys(pool, tenant_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to list request keys: {error}")))?;
    Ok(serde_json::json!({ "keys": keys }))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_request_key(
    pool: &PgPool,
    cert_client: &reqwest::Client,
    cert_service_url: &str,
    cert_service_token: &str,
    code: &str,
    actor_id: uuid::Uuid,
) -> Result<serde_json::Value, ApiError> {
    ensure_tenant_crypto_enabled(cert_client, cert_service_url, cert_service_token).await?;
    let tenant_id = resolve_tenant_id(pool, code).await?;
    let existing = repo::list_request_keys(pool, tenant_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to list request keys: {error}")))?;
    let next_kid = existing
        .iter()
        .filter_map(|row| row.kid.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let kid = next_kid.max(1).to_string();
    let (master_key, integrity_key) =
        load_root_materials(cert_client, cert_service_url, cert_service_token).await?;
    let wrapped = wrap_new_tenant_key(
        &tenant_id.to_string(),
        &kid,
        &master_key,
        &integrity_key,
        COMMERCIAL_ROOT_PROTECTION_WRAPPING_KEY_ID,
    )
    .map_err(|error| ApiError::internal(format!("Failed to wrap request key: {error}")))?;
    let key = repo::insert_request_key(
        pool,
        repo::NewRequestKey {
            tenant_id,
            kid: &kid,
            alg: "hmac-sha256",
            encrypted_material: &wrapped.encrypted_secret_key,
            material_digest: &wrapped.secret_key_digest,
            wrapping_key_id: &wrapped.wrapping_key_id,
            actor_id,
        },
    )
    .await
    .map_err(|error| ApiError::internal(format!("Failed to create request key: {error}")))?;
    Ok(serde_json::json!({
        "kid": key.kid,
        "alg": key.alg,
        "status": key.status,
        "hmacKey": wrapped.tenant_secret_key_b64,
        "shownOnce": true
    }))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn retire_request_key(
    pool: &PgPool,
    code: &str,
    kid: &str,
    actor_id: uuid::Uuid,
) -> Result<serde_json::Value, ApiError> {
    let tenant_id = resolve_tenant_id(pool, code).await?;
    let key = repo::set_request_key_status(pool, tenant_id, kid, "retiring", actor_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to retire request key: {error}")))?
        .ok_or_else(|| ApiError::not_found("tenant_request_key".to_string(), kid.to_string()))?;
    Ok(serde_json::to_value(key).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> UpdateCryptoProfileRequest {
        UpdateCryptoProfileRequest {
            encryption_mode: "full".into(),
            suite: Some(SUITE_TENANT_DEFAULT_2026_11.into()),
            data_alg: None,
            mac_alg: None,
            allow_insecure: false,
            require_signature: false,
            require_request_binding: false,
            signature_version: "hmac-v1".into(),
            nonce_ttl_seconds: 300,
            clock_skew_seconds: 300,
            current_key_version: Some("v1".into()),
        }
    }

    #[test]
    fn recommended_suite_profile_validates() {
        let req = base_request();
        let profile = req.validate_against_profile().expect("valid");
        assert_eq!(profile.mode, TransportMode::Full);
        assert_eq!(
            profile.suite.as_deref(),
            Some(SUITE_TENANT_DEFAULT_2026_11)
        );
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
        req.suite = Some(SUITE_TENANT_DEFAULT_2026_11.into());
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
