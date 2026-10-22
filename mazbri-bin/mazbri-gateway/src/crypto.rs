use crate::gateway::ProxyError;
use crate::gateway::validation::GatewayRequest;
use crate::gateway::validation::GatewayRequestPayload;
use crate::redis_client::RedisClient;
use chrono::Utc;
use mazbri_core::crypto::tenant_key::{
    RootProtectionMaterialBundle, decode_configured_key, unwrap_tenant_key,
    verify_secret_key_digest,
};
use mazbri_core::crypto::transport::{
    DATA_ALGORITHM, SIGNATURE_ALGORITHM, TransportEnvelope, canonical_signature_payload,
    decrypt_aes_256_gcm, encrypt_aes_256_gcm, generate_nonce_base64url, sign_payload,
    validate_transport_algorithms, verify_payload_signature,
};
use mazbri_core::metrics::{MetricsBuilder, REGISTRY, register_collector_once};
use mazbri_core::num::SaturatingFrom;
use prometheus::{CounterVec, HistogramOpts, HistogramVec};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::warn;
use uuid::Uuid;

const IDEMPOTENCY_PROCESSING: &str = r#"{"status":"processing"}"#;

/// Stored under Redis idempotency key after successful first completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    pub status: String,
    #[serde(default)]
    pub http_status: u16,
    #[serde(default)]
    pub body: Vec<u8>,
    #[serde(default)]
    pub content_type: Option<String>,
}

impl IdempotencyRecord {
    #[must_use]
    pub fn done(http_status: u16, body: Vec<u8>, content_type: Option<String>) -> Self {
        Self {
            status: "done".to_string(),
            http_status,
            body,
            content_type,
        }
    }
}

#[derive(Debug)]
pub enum IdempotencyOutcome {
    /// First logical call; proceed and complete later.
    Acquired,
    /// Same fingerprint still processing.
    InProgress,
    /// Prior success; replay body without re-billing.
    Completed(IdempotencyRecord),
}

pub static GATEWAY_CRYPTO_CACHE_TOTAL: std::sync::LazyLock<CounterVec> =
    std::sync::LazyLock::new(|| {
        MetricsBuilder::new().counter(
            "gateway_crypto_cache_total",
            "Gateway tenant crypto cache hits and misses",
            &["cache", "result"],
        )
    });

pub static GATEWAY_CRYPTO_MAINTENANCE_TOTAL: std::sync::LazyLock<CounterVec> =
    std::sync::LazyLock::new(|| {
        MetricsBuilder::new().counter(
            "gateway_crypto_maintenance_total",
            "Gateway tenant crypto warmup and refresh results",
            &["task", "result"],
        )
    });

pub static GATEWAY_CRYPTO_MAINTENANCE_SECONDS: std::sync::LazyLock<HistogramVec> =
    std::sync::LazyLock::new(|| {
        HistogramVec::new(
            HistogramOpts::new(
                "gateway_crypto_maintenance_seconds",
                "Gateway tenant crypto warmup and refresh duration",
            )
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ]),
            &["task", "result"],
        )
        .expect("valid metric definition for gateway tenant crypto maintenance")
    });

pub static GATEWAY_CRYPTO_LOOKUP_SECONDS: std::sync::LazyLock<HistogramVec> =
    std::sync::LazyLock::new(|| {
        HistogramVec::new(
            HistogramOpts::new(
                "gateway_crypto_lookup_seconds",
                "Gateway tenant crypto profile and key lookup duration",
            )
            .buckets(vec![
                0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0,
            ]),
            &["cache", "result"],
        )
        .expect("valid metric definition for gateway tenant crypto lookup")
    });

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_crypto_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(GATEWAY_CRYPTO_CACHE_TOTAL.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(GATEWAY_CRYPTO_MAINTENANCE_TOTAL.clone()),
    )?;
    register_collector_once(
        &REGISTRY,
        Box::new(GATEWAY_CRYPTO_MAINTENANCE_SECONDS.clone()),
    )?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_CRYPTO_LOOKUP_SECONDS.clone()))?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize, sqlx::FromRow)]
pub struct TenantCryptoProfile {
    pub encryption_mode: String,
    pub require_signature: bool,
    pub require_request_binding: bool,
    pub signature_version: String,
    pub current_key_version: Option<String>,
    pub nonce_ttl_seconds: i32,
    pub clock_skew_seconds: i32,
}

#[derive(Debug, Clone, Deserialize, sqlx::FromRow)]
pub struct TenantCryptoKey {
    pub tenant_id: Uuid,
    pub key_version: String,
    pub encrypted_secret_key: String,
    pub secret_key_digest: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, sqlx::FromRow)]
struct TenantCryptoSnapshotRow {
    pub tenant_id: Uuid,
    pub encryption_mode: String,
    pub require_signature: bool,
    pub require_request_binding: bool,
    pub signature_version: String,
    pub current_key_version: Option<String>,
    pub nonce_ttl_seconds: i32,
    pub clock_skew_seconds: i32,
    pub key_version: Option<String>,
    pub encrypted_secret_key: Option<String>,
    pub secret_key_digest: Option<String>,
    pub key_status: Option<String>,
}

#[derive(Debug, Clone, Default)]
/// Per-stage durations in milliseconds.
pub struct TenantCryptoTiming {
    pub transport_profile: u64,
    pub envelope: u64,
    pub preflight: u64,
    pub nonce: u64,
    pub tenant_key: u64,
    pub tenant_key_db: u64,
    pub tenant_key_root_material: u64,
    pub tenant_key_local: u64,
    pub signature: u64,
    pub decrypt: u64,
    pub parse: u64,
    pub transport_total: u64,
}

#[derive(Clone)]
pub struct TenantCryptoState {
    db: Arc<PgPool>,
    profile_cache: moka::sync::Cache<Uuid, TenantCryptoProfile>,
    key_cache: moka::sync::Cache<String, Vec<u8>>,
    root_material_cache: moka::sync::Cache<String, RootProtectionMaterialBundle>,
    root_material_refresh_lock: Arc<tokio::sync::Mutex<()>>,
    http_client: reqwest::Client,
    redis: RedisClient,
    cert_service_url: String,
    cert_service_token: String,
}

impl TenantCryptoState {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn new(
        db: Arc<PgPool>,
        redis: RedisClient,
        cert_service_url: &str,
        cert_service_token: String,
    ) -> anyhow::Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            db,
            profile_cache: moka::sync::Cache::builder()
                .max_capacity(100_000)
                .support_invalidation_closures()
                .time_to_live(Duration::from_secs(300))
                .build(),
            key_cache: moka::sync::Cache::builder()
                .max_capacity(100_000)
                .support_invalidation_closures()
                .time_to_live(Duration::from_mins(15))
                .build(),
            root_material_cache: moka::sync::Cache::builder()
                .max_capacity(1)
                .time_to_live(Duration::from_secs(300))
                .build(),
            root_material_refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
            http_client,
            redis,
            cert_service_url: cert_service_url.to_string(),
            cert_service_token,
        })
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn profile(&self, tenant_id: Uuid) -> Result<TenantCryptoProfile, ProxyError> {
        let lookup_started_at = Instant::now();
        if let Some(profile) = self.profile_cache.get(&tenant_id) {
            record_crypto_cache_result("profile", "hit");
            record_crypto_lookup_duration("profile", "hit", lookup_started_at.elapsed());
            return Ok(profile);
        }
        record_crypto_cache_result("profile", "miss");
        let snapshot = self.load_profile_snapshot(tenant_id).await?;
        let Some(snapshot) = snapshot else {
            let profile = TenantCryptoProfile {
                encryption_mode: "none".to_string(),
                require_signature: true,
                require_request_binding: false,
                signature_version: "hmac-v1".to_string(),
                current_key_version: None,
                nonce_ttl_seconds: 300,
                clock_skew_seconds: 300,
            };
            self.profile_cache.insert(tenant_id, profile.clone());
            record_crypto_lookup_duration("profile", "miss", lookup_started_at.elapsed());
            return Ok(profile);
        };
        let profile = Self::profile_from_snapshot(&snapshot);
        self.profile_cache.insert(tenant_id, profile.clone());
        if let Some(key_row) = Self::key_from_snapshot(&snapshot)
            && let Err(error) = self.materialize_key_row_with_root(&key_row).await
        {
            warn!(
                tenant_id = % tenant_id,
                key_version = % key_row.key_version,
                error = % error,
                "Tenant crypto current key warmup failed"
            );
        }
        record_crypto_lookup_duration("profile", "miss", lookup_started_at.elapsed());
        Ok(profile)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn warm_start(&self) -> Result<(), ProxyError> {
        let mut last_error: Option<ProxyError> = None;
        let root_started_at = Instant::now();
        if let Err(error) = self.refresh_root_material_cache().await {
            record_crypto_maintenance("root_material_warmup", "error", root_started_at.elapsed());
            warn!(error = %error, "Tenant crypto root material warmup failed");
            last_error = Some(error);
        } else {
            record_crypto_maintenance("root_material_warmup", "success", root_started_at.elapsed());
        }
        let snapshot_started_at = Instant::now();
        if let Err(error) = self.refresh_active_crypto_snapshot_cache().await {
            record_crypto_maintenance("snapshot_warmup", "error", snapshot_started_at.elapsed());
            warn!(error = %error, "Tenant crypto snapshot warmup failed");
            last_error = Some(error);
        } else {
            record_crypto_maintenance("snapshot_warmup", "success", snapshot_started_at.elapsed());
        }
        if let Some(error) = last_error {
            return Err(error);
        }
        Ok(())
    }

    #[must_use]
    pub fn spawn_background_tasks(&self) -> Vec<tokio::task::JoinHandle<()>> {
        let snapshot_state = self.clone();
        let root_state = self.clone();
        vec![
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    let started_at = Instant::now();
                    match snapshot_state.refresh_active_crypto_snapshot_cache().await {
                        Ok(()) => {
                            record_crypto_maintenance(
                                "snapshot_refresh",
                                "success",
                                started_at.elapsed(),
                            );
                        }
                        Err(error) => {
                            record_crypto_maintenance(
                                "snapshot_refresh",
                                "error",
                                started_at.elapsed(),
                            );
                            warn!(error = %error, "Tenant crypto snapshot refresh failed");
                        }
                    }
                }
            }),
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(240));
                loop {
                    interval.tick().await;
                    let started_at = Instant::now();
                    match root_state.refresh_root_material_cache().await {
                        Ok(()) => {
                            record_crypto_maintenance(
                                "root_material_refresh",
                                "success",
                                started_at.elapsed(),
                            );
                        }
                        Err(error) => {
                            record_crypto_maintenance(
                                "root_material_refresh",
                                "error",
                                started_at.elapsed(),
                            );
                            warn!(error = %error, "Tenant crypto root material refresh failed");
                        }
                    }
                }
            }),
        ]
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn refresh_active_crypto_snapshot_cache(&self) -> Result<(), ProxyError> {
        let snapshots = sqlx::query_as::<_, TenantCryptoSnapshotRow>(
            r"
            SELECT p.tenant_id, p.encryption_mode, p.require_signature, p.require_request_binding,
                p.signature_version, p.current_key_version,
                p.nonce_ttl_seconds, p.clock_skew_seconds, k.key_version, k.encrypted_secret_key,
                k.secret_key_digest, k.status AS key_status
            FROM tenant_crypto_profiles p
            LEFT JOIN tenant_crypto_keys k
                ON k.tenant_id = p.tenant_id AND k.key_version = p.current_key_version
            WHERE p.encryption_mode <> 'none'
            ORDER BY p.tenant_id
            ",
        )
        .fetch_all(self.db.as_ref())
        .await
        .map_err(|error| {
            ProxyError::InternalError(format!("crypto snapshot refresh failed: {error}"))
        })?;
        let mut active_tenants = HashSet::with_capacity(snapshots.len());
        let root_materials = match self.root_materials().await {
            Ok(materials) => match decode_root_materials(&materials) {
                Ok(decoded) => Some(decoded),
                Err(error) => {
                    warn!(error = %error, "Tenant crypto root material decode failed during snapshot refresh");
                    None
                }
            },
            Err(error) => {
                warn!(error = %error, "Tenant crypto root material lookup failed during snapshot refresh");
                None
            }
        };
        for snapshot in snapshots {
            active_tenants.insert(snapshot.tenant_id);
            let profile = Self::profile_from_snapshot(&snapshot);
            self.profile_cache.insert(snapshot.tenant_id, profile);
            if let Some(key_row) = Self::key_from_snapshot(&snapshot) {
                if let Some((master_key, integrity_key)) = root_materials.as_ref() {
                    match Self::materialize_key_row(&key_row, master_key, integrity_key) {
                        Ok(key) => {
                            self.key_cache.insert(
                                Self::cache_key(&key_row.tenant_id, &key_row.key_version),
                                key,
                            );
                        }
                        Err(error) => {
                            warn!(
                                tenant_id = % key_row.tenant_id,
                                key_version = % key_row.key_version,
                                error = % error,
                                "Tenant crypto key refresh failed"
                            );
                        }
                    }
                } else {
                    warn!(
                        tenant_id = % snapshot.tenant_id,
                        key_version = % key_row.key_version,
                        "Tenant crypto key refresh skipped because root material cache was unavailable"
                    );
                }
            } else if matches!(snapshot.encryption_mode.as_str(), "request" | "full")
                && snapshot.current_key_version.is_some()
            {
                warn!(
                    tenant_id = % snapshot.tenant_id,
                    current_key_version = ? snapshot.current_key_version,
                    "Tenant crypto current key row was not found during snapshot refresh"
                );
            }
        }
        let active_tenants = Arc::new(active_tenants);
        let invalidate_result = self.profile_cache.invalidate_entries_if({
            let active_tenants = Arc::clone(&active_tenants);
            move |tenant_id, _| !active_tenants.contains(tenant_id)
        });
        if let Err(error) = invalidate_result {
            warn!(
                error = % error,
                "Tenant crypto profile cache invalidation closure is unavailable"
            );
        }
        Ok(())
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn refresh_root_material_cache(&self) -> Result<(), ProxyError> {
        let cache_key = "active".to_string();
        let _guard = self.root_material_refresh_lock.lock().await;
        let materials = mazbri_core::crypto::tenant_key::fetch_root_protection_material_bundle(
            &self.http_client,
            &self.cert_service_url,
            &self.cert_service_token,
        )
        .await
        .map_err(|error| {
            ProxyError::InternalError(format!("tenant crypto root material fetch failed: {error}"))
        })?;
        self.root_material_cache
            .insert(cache_key, materials.clone());
        Ok(())
    }

    async fn load_profile_snapshot(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<TenantCryptoSnapshotRow>, ProxyError> {
        sqlx::query_as::<_, TenantCryptoSnapshotRow>(
            r"
            SELECT p.tenant_id, p.encryption_mode, p.require_signature, p.require_request_binding,
                p.signature_version, p.current_key_version,
                p.nonce_ttl_seconds, p.clock_skew_seconds, k.key_version, k.encrypted_secret_key,
                k.secret_key_digest, k.status AS key_status
            FROM tenant_crypto_profiles p
            LEFT JOIN tenant_crypto_keys k
                ON k.tenant_id = p.tenant_id AND k.key_version = p.current_key_version
            WHERE p.tenant_id = $1
            ",
        )
        .bind(tenant_id)
        .fetch_optional(self.db.as_ref())
        .await
        .map_err(|error| {
            ProxyError::InternalError(format!("crypto profile lookup failed: {error}"))
        })
    }

    fn profile_from_snapshot(snapshot: &TenantCryptoSnapshotRow) -> TenantCryptoProfile {
        TenantCryptoProfile {
            encryption_mode: snapshot.encryption_mode.clone(),
            require_signature: snapshot.require_signature,
            require_request_binding: snapshot.require_request_binding,
            signature_version: snapshot.signature_version.clone(),
            current_key_version: snapshot.current_key_version.clone(),
            nonce_ttl_seconds: snapshot.nonce_ttl_seconds,
            clock_skew_seconds: snapshot.clock_skew_seconds,
        }
    }

    fn key_from_snapshot(snapshot: &TenantCryptoSnapshotRow) -> Option<TenantCryptoKey> {
        Some(TenantCryptoKey {
            tenant_id: snapshot.tenant_id,
            key_version: snapshot.key_version.clone()?,
            encrypted_secret_key: snapshot.encrypted_secret_key.clone()?,
            secret_key_digest: snapshot.secret_key_digest.clone()?,
            status: snapshot.key_status.clone()?,
        })
    }

    fn cache_key(tenant_id: &Uuid, key_version: &str) -> String {
        format!("{tenant_id}:{key_version}")
    }

    fn materialize_key_row(
        row: &TenantCryptoKey,
        master_key: &[u8],
        integrity_key: &[u8],
    ) -> Result<Vec<u8>, ProxyError> {
        if !matches!(row.status.as_str(), "active" | "deprecated") {
            return Err(ProxyError::Forbidden(
                "Invalid encrypted request".to_string(),
            ));
        }
        let key = unwrap_tenant_key(
            &row.encrypted_secret_key,
            &row.tenant_id.to_string(),
            &row.key_version,
            master_key,
        )
        .map_err(|_| ProxyError::Forbidden("Invalid encrypted request".to_string()))?;
        let digest_ok = verify_secret_key_digest(
            integrity_key,
            &row.tenant_id.to_string(),
            &row.key_version,
            &key,
            &row.secret_key_digest,
        )
        .map_err(|_| ProxyError::Forbidden("Invalid encrypted request".to_string()))?;
        if !digest_ok {
            return Err(ProxyError::Forbidden(
                "Invalid encrypted request".to_string(),
            ));
        }
        Ok(key)
    }

    async fn materialize_key_row_with_root(
        &self,
        row: &TenantCryptoKey,
    ) -> Result<Vec<u8>, ProxyError> {
        let materials = self.root_materials().await?;
        let (master_key, integrity_key) = decode_root_materials(&materials)?;
        Self::materialize_key_row(row, &master_key, &integrity_key)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn decrypt_request_value(
        &self,
        tenant_id: Uuid,
        body: GatewayRequestPayload,
        http_path: &str,
    ) -> Result<
        (
            GatewayRequest,
            TenantCryptoProfile,
            bool,
            TenantCryptoTiming,
        ),
        ProxyError,
    > {
        let started_at = Instant::now();
        let mut timing = TenantCryptoTiming::default();
        let profile_started_at = Instant::now();
        let profile = self.profile(tenant_id).await?;
        timing.transport_profile = mazbri_core::num::millis_u64(profile_started_at.elapsed());
        match body {
            GatewayRequestPayload::Plain(req) => {
                if profile.encryption_mode != "none" {
                    return Err(ProxyError::BadRequest(
                        "Encrypted request envelope is required".to_string(),
                    ));
                }
                timing.transport_total = mazbri_core::num::millis_u64(started_at.elapsed());
                Ok((req, profile, false, timing))
            }
            GatewayRequestPayload::Encrypted(envelope) => {
                if profile.encryption_mode == "none" {
                    return Err(ProxyError::BadRequest(
                        "Invalid proxy request body".to_string(),
                    ));
                }
                let tenant_id_str = tenant_id.to_string();
                let preflight_started_at = Instant::now();
                validate_transport_algorithms(&envelope).map_err(|_| {
                    ProxyError::BadRequest("Invalid encrypted request envelope".to_string())
                })?;
                validate_timestamp(&envelope, profile.clock_skew_seconds)?;
                timing.preflight = mazbri_core::num::millis_u64(preflight_started_at.elapsed());
                self.check_nonce(tenant_id, &envelope, profile.nonce_ttl_seconds, &mut timing)
                    .await?;
                let tenant_key = self
                    .tenant_key(tenant_id, &envelope.key_version, &mut timing)
                    .await?;
                let signature_started_at = Instant::now();
                if profile.require_signature {
                    let payload = canonical_signature_payload(&envelope, &tenant_id_str, http_path);
                    verify_payload_signature(&tenant_key, &payload, &envelope.signature).map_err(
                        |_| ProxyError::Forbidden("Invalid encrypted request".to_string()),
                    )?;
                }
                timing.signature = mazbri_core::num::millis_u64(signature_started_at.elapsed());
                let decrypt_started_at = Instant::now();
                let plaintext = decrypt_aes_256_gcm(
                    &tenant_key,
                    &envelope.iv,
                    &envelope.ciphertext,
                    &envelope.tag,
                    tenant_id_str.as_bytes(),
                )
                .map_err(|_| {
                    ProxyError::BadRequest("Invalid encrypted request body".to_string())
                })?;
                timing.decrypt = mazbri_core::num::millis_u64(decrypt_started_at.elapsed());
                let parse_started_at = Instant::now();
                let req = serde_json::from_slice::<GatewayRequest>(&plaintext).map_err(|_| {
                    ProxyError::BadRequest("Decrypted request body is invalid".to_string())
                })?;
                timing.parse = mazbri_core::num::millis_u64(parse_started_at.elapsed());
                timing.transport_total = mazbri_core::num::millis_u64(started_at.elapsed());
                Ok((req, profile, true, timing))
            }
        }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn encrypt_response_bytes(
        &self,
        tenant_id: Uuid,
        profile: &TenantCryptoProfile,
        plaintext: &[u8],
        http_path: &str,
    ) -> Result<TransportEnvelope, ProxyError> {
        let key_version = profile.current_key_version.as_deref().ok_or_else(|| {
            ProxyError::InternalError("tenant crypto current key is not configured".to_string())
        })?;
        let mut timing = TenantCryptoTiming::default();
        let tenant_key = self.tenant_key(tenant_id, key_version, &mut timing).await?;
        let timestamp = Utc::now().timestamp_millis();
        let nonce = generate_nonce_base64url();
        let tenant_id_str = tenant_id.to_string();
        let encrypted = encrypt_aes_256_gcm(&tenant_key, plaintext, tenant_id_str.as_bytes())
            .map_err(|error| {
                ProxyError::InternalError(format!("failed to encrypt response: {error}"))
            })?;
        let mut envelope = TransportEnvelope {
            encrypted: true,
            key_version: key_version.to_string(),
            algorithm: DATA_ALGORITHM.to_string(),
            signature_algorithm: SIGNATURE_ALGORITHM.to_string(),
            timestamp,
            nonce,
            iv: encrypted.iv,
            ciphertext: encrypted.ciphertext,
            tag: encrypted.tag,
            signature: String::new(),
        };
        let payload = canonical_signature_payload(&envelope, &tenant_id_str, http_path);
        envelope.signature = sign_payload(&tenant_key, &payload).map_err(|error| {
            ProxyError::InternalError(format!("failed to sign encrypted response: {error}"))
        })?;
        Ok(envelope)
    }

    async fn tenant_key(
        &self,
        tenant_id: Uuid,
        key_version: &str,
        timing: &mut TenantCryptoTiming,
    ) -> Result<Vec<u8>, ProxyError> {
        let started_at = Instant::now();
        let cache_key = Self::cache_key(&tenant_id, key_version);
        if let Some(key) = self.key_cache.get(&cache_key) {
            record_crypto_cache_result("key", "hit");
            record_crypto_lookup_duration("key", "hit", started_at.elapsed());
            timing.tenant_key = mazbri_core::num::millis_u64(started_at.elapsed());
            return Ok(key);
        }
        record_crypto_cache_result("key", "miss");
        let db_started_at = Instant::now();
        let row = sqlx::query_as::<_, TenantCryptoKey>(
            r"
            SELECT tenant_id, key_version, encrypted_secret_key, secret_key_digest, status
            FROM tenant_crypto_keys
            WHERE tenant_id = $1 AND key_version = $2
            ",
        )
        .bind(tenant_id)
        .bind(key_version)
        .fetch_optional(self.db.as_ref())
        .await;
        timing.tenant_key_db = mazbri_core::num::millis_u64(db_started_at.elapsed());
        let row = row
            .map_err(|error| {
                ProxyError::InternalError(format!("crypto key lookup failed: {error}"))
            })?
            .ok_or_else(|| ProxyError::Forbidden("Invalid encrypted request".to_string()))?;
        if !matches!(row.status.as_str(), "active" | "deprecated") {
            return Err(ProxyError::Forbidden(
                "Invalid encrypted request".to_string(),
            ));
        }
        let root_material_started_at = Instant::now();
        let materials = self.root_materials().await;
        timing.tenant_key_root_material =
            mazbri_core::num::millis_u64(root_material_started_at.elapsed());
        let materials = materials?;
        let local_started_at = Instant::now();
        let (master_key, integrity_key) = decode_root_materials(&materials)?;
        let key = Self::materialize_key_row(&row, &master_key, &integrity_key)?;
        timing.tenant_key_local = mazbri_core::num::millis_u64(local_started_at.elapsed());
        timing.tenant_key = mazbri_core::num::millis_u64(started_at.elapsed());
        self.key_cache.insert(cache_key, key.clone());
        record_crypto_lookup_duration("key", "miss", started_at.elapsed());
        Ok(key)
    }

    async fn root_materials(&self) -> Result<RootProtectionMaterialBundle, ProxyError> {
        let cache_key = "active".to_string();
        if let Some(materials) = self.root_material_cache.get(&cache_key) {
            return Ok(materials);
        }
        let _guard = self.root_material_refresh_lock.lock().await;
        if let Some(materials) = self.root_material_cache.get(&cache_key) {
            return Ok(materials);
        }
        let materials = mazbri_core::crypto::tenant_key::fetch_root_protection_material_bundle(
            &self.http_client,
            &self.cert_service_url,
            &self.cert_service_token,
        )
        .await
        .map_err(|error| {
            ProxyError::InternalError(format!("tenant crypto root material fetch failed: {error}"))
        })?;
        self.root_material_cache
            .insert(cache_key, materials.clone());
        Ok(materials)
    }

    /// Acquire fingerprint lock or return prior result.
    ///
    /// - `Acquired`: first call; caller must complete and `complete_idempotency`.
    /// - `InProgress`: same fingerprint still running → 409.
    /// - `Completed`: prior success body; return without re-billing.
    ///
    /// Redis failure policy: **fail-open** (returns `Acquired`) so transient Redis
    /// outage does not drop traffic; risk is weaker dedupe under Redis fault.
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn check_idempotency(
        &self,
        fingerprint: &str,
        ttl_secs: u64,
    ) -> Result<IdempotencyOutcome, ProxyError> {
        let ttl_secs = usize::saturating_from(ttl_secs.max(1));
        let key = self.redis.idempotency_key(fingerprint);
        match self
            .redis
            .set_nx_ex(&key, IDEMPOTENCY_PROCESSING.as_bytes(), ttl_secs)
            .await
        {
            Ok(true) => Ok(IdempotencyOutcome::Acquired),
            Ok(false) => match self.redis.get_bytes(&key).await {
                Ok(Some(bytes)) => match serde_json::from_slice::<IdempotencyRecord>(&bytes) {
                    Ok(record) if record.status == "done" => {
                        Ok(IdempotencyOutcome::Completed(record))
                    }
                    _ => Ok(IdempotencyOutcome::InProgress),
                },
                Ok(None) => {
                    // Race: key expired between NX miss and GET → re-acquire
                    match self
                        .redis
                        .set_nx_ex(&key, IDEMPOTENCY_PROCESSING.as_bytes(), ttl_secs)
                        .await
                    {
                        Ok(true) => Ok(IdempotencyOutcome::Acquired),
                        Ok(false) => Ok(IdempotencyOutcome::InProgress),
                        Err(error) => {
                            tracing::warn!(
                                fingerprint = %fingerprint,
                                error = %error,
                                "Idempotency re-acquire failed; fail-open"
                            );
                            Ok(IdempotencyOutcome::Acquired)
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        fingerprint = %fingerprint,
                        error = %error,
                        "Idempotency GET failed; fail-open"
                    );
                    Ok(IdempotencyOutcome::Acquired)
                }
            },
            Err(error) => {
                tracing::warn!(
                    fingerprint = %fingerprint,
                    error = %error,
                    "Idempotency SET NX failed; fail-open"
                );
                Ok(IdempotencyOutcome::Acquired)
            }
        }
    }

    /// Store successful response for fingerprint so transport retries replay without re-charge.
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn complete_idempotency(
        &self,
        fingerprint: &str,
        ttl_secs: u64,
        record: &IdempotencyRecord,
    ) -> Result<(), ProxyError> {
        let ttl_secs = usize::saturating_from(ttl_secs.max(1));
        let key = self.redis.idempotency_key(fingerprint);
        let bytes = serde_json::to_vec(record).map_err(|error| {
            ProxyError::InternalError(format!("idempotency serialize failed: {error}"))
        })?;
        self.redis
            .set_ex_bytes(&key, &bytes, ttl_secs)
            .await
            .map_err(|error| {
                ProxyError::InternalError(format!("idempotency complete redis failed: {error}"))
            })
    }

    async fn check_nonce(
        &self,
        tenant_id: Uuid,
        envelope: &TransportEnvelope,
        ttl_seconds: i32,
        timing: &mut TenantCryptoTiming,
    ) -> Result<(), ProxyError> {
        let started_at = Instant::now();
        let inserted = retry_once("SET NX EX", || async {
            self.redis
                .set_nonce_if_absent(
                    tenant_id,
                    &envelope.key_version,
                    &envelope.nonce,
                    ttl_seconds,
                )
                .await
                .map_err(|error| {
                    ProxyError::InternalError(format!("nonce redis operation failed: {error}"))
                })
        })
        .await;
        timing.nonce = mazbri_core::num::millis_u64(started_at.elapsed());
        let inserted = inserted?;
        if !inserted {
            return Err(ProxyError::Conflict(
                "Duplicate encrypted request".to_string(),
            ));
        }
        Ok(())
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn redis_health(&self) -> anyhow::Result<()> {
        self.redis
            .ping()
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn consume_binding_nonce(
        &self,
        prefix: &str,
        tenant_id: Uuid,
        nonce: &str,
        ttl_seconds: i32,
    ) -> Result<bool, ProxyError> {
        self.redis
            .set_gateway_binding_nonce_if_absent(prefix, tenant_id, nonce, ttl_seconds)
            .await
            .map_err(|error| {
                ProxyError::InternalError(format!("binding nonce redis operation failed: {error}"))
            })
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn request_hmac_key(
        &self,
        tenant_id: Uuid,
        kid: &str,
    ) -> Result<Option<Vec<u8>>, ProxyError> {
        let cache_key = format!("reqkey:{tenant_id}:{kid}");
        if let Some(key) = self.key_cache.get(&cache_key) {
            return Ok(Some(key));
        }
        let row = sqlx::query_as::<_, TenantRequestKeyRow>(
            r"
            SELECT tenant_id, kid, alg, encrypted_material, material_digest, status,
                not_before, not_after
            FROM tenant_request_keys
            WHERE tenant_id = $1 AND kid = $2
            ",
        )
        .bind(tenant_id)
        .bind(kid)
        .fetch_optional(self.db.as_ref())
        .await
        .map_err(|error| {
            ProxyError::InternalError(format!("request key lookup failed: {error}"))
        })?;
        let Some(row) = row else {
            return Ok(None);
        };
        if !matches!(row.status.as_str(), "active" | "retiring") {
            return Ok(None);
        }
        let now = Utc::now();
        if let Some(not_before) = row.not_before
            && now < not_before
        {
            return Ok(None);
        }
        if let Some(not_after) = row.not_after
            && now >= not_after
        {
            return Ok(None);
        }
        if row.alg != "hmac-sha256" {
            return Ok(None);
        }
        let key = self.unwrap_request_key(&row).await?;
        self.key_cache.insert(cache_key, key.clone());
        Ok(Some(key))
    }

    async fn unwrap_request_key(&self, row: &TenantRequestKeyRow) -> Result<Vec<u8>, ProxyError> {
        let materials = self.root_materials().await?;
        let (master_key, integrity_key) = decode_root_materials(&materials)?;
        let key = unwrap_tenant_key(
            &row.encrypted_material,
            &row.tenant_id.to_string(),
            &row.kid,
            &master_key,
        )
        .map_err(|_| ProxyError::Forbidden("Invalid request binding key".to_string()))?;
        let digest_ok = verify_secret_key_digest(
            &integrity_key,
            &row.tenant_id.to_string(),
            &row.kid,
            &key,
            &row.material_digest,
        )
        .map_err(|_| ProxyError::Forbidden("Invalid request binding key".to_string()))?;
        if !digest_ok {
            return Err(ProxyError::Forbidden(
                "Invalid request binding key".to_string(),
            ));
        }
        Ok(key)
    }
}

#[derive(Debug, Clone, Deserialize, sqlx::FromRow)]
struct TenantRequestKeyRow {
    pub tenant_id: Uuid,
    pub kid: String,
    pub alg: String,
    pub encrypted_material: String,
    pub material_digest: String,
    pub status: String,
    pub not_before: Option<chrono::DateTime<Utc>>,
    pub not_after: Option<chrono::DateTime<Utc>>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn load_platform_hs256_jwt_keys(
    db: &PgPool,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    let rows = sqlx::query_as::<_, PlatformSigningKeyRow>(
        r"
        SELECT kid, material, not_before, not_after
        FROM platform_signing_keys
        WHERE purpose = 'jwt' AND alg = 'HS256' AND status IN ('active', 'retiring')
        ",
    )
    .fetch_all(db)
    .await?;
    let now = Utc::now();
    Ok(rows
        .into_iter()
        .filter(|row| {
            row.not_before.is_none_or(|ts| now >= ts) && row.not_after.is_none_or(|ts| now < ts)
        })
        .map(|row| (row.kid, row.material))
        .collect())
}

#[derive(Debug, Clone, Deserialize, sqlx::FromRow)]
struct PlatformSigningKeyRow {
    pub kid: String,
    pub material: String,
    pub not_before: Option<chrono::DateTime<Utc>>,
    pub not_after: Option<chrono::DateTime<Utc>>,
}

fn decode_root_materials(
    materials: &RootProtectionMaterialBundle,
) -> Result<(Vec<u8>, Vec<u8>), ProxyError> {
    let master_key = decode_configured_key(&materials.master_key).map_err(|error| {
        ProxyError::InternalError(format!("tenant crypto master key error: {error}"))
    })?;
    let integrity_key = decode_configured_key(&materials.integrity_key).map_err(|error| {
        ProxyError::InternalError(format!("tenant crypto integrity key error: {error}"))
    })?;
    Ok((master_key, integrity_key))
}

fn record_crypto_cache_result(cache: &'static str, result: &'static str) {
    GATEWAY_CRYPTO_CACHE_TOTAL
        .with_label_values(&[cache, result])
        .inc();
}

fn record_crypto_lookup_duration(cache: &'static str, result: &'static str, duration: Duration) {
    GATEWAY_CRYPTO_LOOKUP_SECONDS
        .with_label_values(&[cache, result])
        .observe(duration.as_secs_f64());
}

fn record_crypto_maintenance(task: &'static str, result: &'static str, duration: Duration) {
    GATEWAY_CRYPTO_MAINTENANCE_TOTAL
        .with_label_values(&[task, result])
        .inc();
    GATEWAY_CRYPTO_MAINTENANCE_SECONDS
        .with_label_values(&[task, result])
        .observe(duration.as_secs_f64());
}

async fn retry_once<T, E, F, Fut>(op: &'static str, mut f: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    match f().await {
        Ok(value) => Ok(value),
        Err(first_error) => {
            warn!(op = op, error = %first_error, "Redis operation failed; retrying once");
            tokio::time::sleep(Duration::from_millis(25)).await;
            f().await
        }
    }
}

fn validate_timestamp(
    envelope: &TransportEnvelope,
    clock_skew_seconds: i32,
) -> Result<(), ProxyError> {
    let now = Utc::now().timestamp_millis();
    let skew_ms = i64::from(clock_skew_seconds) * 1000;
    if (now - envelope.timestamp).abs() > skew_ms {
        return Err(ProxyError::Forbidden(
            "Invalid encrypted request".to_string(),
        ));
    }
    Ok(())
}
