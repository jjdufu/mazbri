use crate::gateway::ProxyError;
use crate::gateway::validation::GatewayRequest;
use crate::gateway::validation::GatewayRequestPayload;
use crate::redis_client::RedisClient;
use chrono::Utc;
use mazbri_core::crypto::request_binding::{
    binding_key_ref, parse_binding_alg, public_key_digest_scope,
};
use mazbri_core::crypto::tenant_key::{
    RootProtectionMaterialBundle, decode_configured_key, unwrap_tenant_key,
    verify_secret_key_digest,
};
use mazbri_core::crypto::transport::{
    SealEnvelopeParams, TransportEnvelope, open_envelope, seal_envelope,
};
use mazbri_core::crypto::{
    AlgorithmId, AlgorithmKind, AlgorithmRegistry, CryptoError, KeyRef, KeySelection, TenantTransportProfile,
    TransportMode, parse_algorithm_id,
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
    pub profile_version: i32,
    pub suite: Option<String>,
    pub data_alg: Option<String>,
    pub mac_alg: Option<String>,
    pub require_signature: bool,
    pub allow_insecure: bool,
    pub require_request_binding: bool,
    pub signature_version: String,
    pub current_key_version: Option<String>,
    pub nonce_ttl_seconds: i32,
    pub clock_skew_seconds: i32,
}

impl TenantCryptoProfile {
    /// 将 DB 行映射为 [`TenantTransportProfile`]（算法 id 为闭集 kebab-case）。
    ///
    /// # Errors
    ///
    /// mode / 算法字符串非法、legacy 未放行时返回清晰的 `ProxyError`。
    pub fn transport_profile(&self) -> Result<TenantTransportProfile, ProxyError> {
        let mode = parse_transport_mode(&self.encryption_mode)?;
        let data_alg = parse_optional_alg(self.data_alg.as_deref(), "data_alg")?;
        let mac_alg = parse_optional_alg(self.mac_alg.as_deref(), "mac_alg")?;
        let key = self.current_key_version.as_ref().map(|version| KeySelection::Latest {
            key_id: version.clone(),
        });
        let profile_version = u32::try_from(self.profile_version).map_err(|_| {
            ProxyError::InternalError("tenant crypto profile_version must be >= 0".to_string())
        })?;
        let nonce_ttl_secs = u32::try_from(self.nonce_ttl_seconds).map_err(|_| {
            ProxyError::InternalError("tenant crypto nonce_ttl_seconds must be >= 0".to_string())
        })?;
        let clock_skew_secs = u32::try_from(self.clock_skew_seconds).map_err(|_| {
            ProxyError::InternalError("tenant crypto clock_skew_seconds must be >= 0".to_string())
        })?;
        Ok(TenantTransportProfile {
            version: if profile_version == 0 {
                1
            } else {
                profile_version
            },
            mode,
            suite: self.suite.clone(),
            data_alg,
            mac_alg,
            key,
            require_signature: self.require_signature,
            allow_insecure: self.allow_insecure,
            binding: None,
            nonce_ttl_secs,
            clock_skew_secs,
        })
    }
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
    pub profile_version: i32,
    pub suite: Option<String>,
    pub data_alg: Option<String>,
    pub mac_alg: Option<String>,
    pub require_signature: bool,
    pub allow_insecure: bool,
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
    request_key_cache: moka::sync::Cache<(Uuid, String), Arc<RequestBindingKey>>,
    root_material_cache: moka::sync::Cache<String, RootProtectionMaterialBundle>,
    root_material_refresh_lock: Arc<tokio::sync::Mutex<()>>,
    http_client: reqwest::Client,
    redis: RedisClient,
    cert_service_url: String,
    cert_service_token: String,
    registry: AlgorithmRegistry,
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
            // 请求绑定密钥：已解析的 KeyRef（RSA/SM2 解析开销大，按 kid 缓存）；
            // TTL 较短，使 retire / 过期在 1 分钟内生效。
            request_key_cache: moka::sync::Cache::builder()
                .max_capacity(100_000)
                .time_to_live(Duration::from_mins(1))
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
            registry: AlgorithmRegistry::default_registry(),
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
                profile_version: 1,
                suite: None,
                data_alg: None,
                mac_alg: None,
                require_signature: false,
                allow_insecure: false,
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
            SELECT p.tenant_id, p.encryption_mode, p.profile_version, p.suite, p.data_alg, p.mac_alg,
                p.require_signature, p.allow_insecure, p.require_request_binding,
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
            SELECT p.tenant_id, p.encryption_mode, p.profile_version, p.suite, p.data_alg, p.mac_alg,
                p.require_signature, p.allow_insecure, p.require_request_binding,
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
            profile_version: snapshot.profile_version,
            suite: snapshot.suite.clone(),
            data_alg: snapshot.data_alg.clone(),
            mac_alg: snapshot.mac_alg.clone(),
            require_signature: snapshot.require_signature,
            allow_insecure: snapshot.allow_insecure,
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
                        "Encrypted envelope is not allowed when encryption_mode=none".to_string(),
                    ));
                }
                let tenant_id_str = tenant_id.to_string();
                let transport = profile.transport_profile()?;
                let preflight_started_at = Instant::now();
                validate_timestamp(&envelope, profile.clock_skew_seconds)?;
                timing.preflight = mazbri_core::num::millis_u64(preflight_started_at.elapsed());
                self.check_nonce(tenant_id, &envelope, profile.nonce_ttl_seconds, &mut timing)
                    .await?;
                let tenant_key = self
                    .tenant_key(tenant_id, &envelope.key_version, &mut timing)
                    .await?;
                let mac_key = envelope
                    .mac_alg
                    .as_ref()
                    .map(|_| tenant_key.as_slice());
                let signature_started_at = Instant::now();
                let decrypt_started_at = Instant::now();
                let plaintext = open_envelope(
                    &transport,
                    &self.registry,
                    &tenant_key,
                    mac_key,
                    &tenant_id_str,
                    http_path,
                    &envelope,
                )
                .map_err(map_transport_crypto_error)?;
                timing.signature = mazbri_core::num::millis_u64(signature_started_at.elapsed());
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
        if profile.encryption_mode != "full" {
            return Err(ProxyError::InternalError(
                "response encryption requires encryption_mode=full".to_string(),
            ));
        }
        let key_version = profile.current_key_version.as_deref().ok_or_else(|| {
            ProxyError::InternalError("tenant crypto current key is not configured".to_string())
        })?;
        let transport = profile.transport_profile()?;
        let mut timing = TenantCryptoTiming::default();
        let tenant_key = self.tenant_key(tenant_id, key_version, &mut timing).await?;
        let tenant_id_str = tenant_id.to_string();
        let (_, mac_alg) = transport.effective_algs().map_err(map_crypto_profile_error)?;
        let mac_key = mac_alg.map(|_| tenant_key.as_slice());
        seal_envelope(SealEnvelopeParams {
            profile: &transport,
            registry: &self.registry,
            data_key: &tenant_key,
            mac_key,
            key_version,
            tenant_id: &tenant_id_str,
            http_path,
            plaintext,
            timestamp: Utc::now().timestamp(),
        })
        .map_err(map_transport_crypto_error)
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
    /// Redis failure policy: **fail-closed** — any Redis error returns
    /// `ProxyError::IdempotencyUnavailable` (503 + Retry-After, code
    /// `IDEMPOTENCY_UNAVAILABLE`), logged and counted. Only keyed requests call this.
    ///
    /// # Errors
    ///
    /// Redis 不可用时返回 `ProxyError::IdempotencyUnavailable`。
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
                            Err(idempotency_unavailable("re-acquire", fingerprint, &error))
                        }
                    }
                }
                Err(error) => Err(idempotency_unavailable("GET", fingerprint, &error)),
            },
            Err(error) => Err(idempotency_unavailable("SET NX", fingerprint, &error)),
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

    /// 算法注册表。
    #[must_use]
    pub const fn registry(&self) -> &AlgorithmRegistry {
        &self.registry
    }

    /// 按 (tenant, kid) 取可用的请求绑定密钥（HMAC 解包 / 公钥校验完整性摘要后解析）。
    ///
    /// kid 不存在、非 active/retiring、不在有效期、算法不在绑定闭集时返回 `Ok(None)`。
    ///
    /// # Errors
    ///
    /// 数据库 / 根材料失败返回 `InternalError`；密钥材料被篡改或非法返回 `Forbidden`。
    pub async fn request_binding_key(
        &self,
        tenant_id: Uuid,
        kid: &str,
    ) -> Result<Option<Arc<RequestBindingKey>>, ProxyError> {
        let cache_key = (tenant_id, kid.to_owned());
        if let Some(key) = self.request_key_cache.get(&cache_key) {
            return Ok(Some(key));
        }
        let row = sqlx::query_as::<_, TenantRequestKeyRow>(
            r"
            SELECT tenant_id, kid, alg, encrypted_material, public_key, material_digest, status,
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
        if !row.usable_at(Utc::now()) {
            return Ok(None);
        }
        let Ok(alg) = parse_binding_alg(&row.alg) else {
            return Ok(None);
        };
        let material = self.request_key_material(&row, alg).await?;
        let key = binding_key_ref(alg, &material)
            .map_err(|_| ProxyError::Forbidden("Invalid request binding key".to_string()))?;
        let key = Arc::new(RequestBindingKey { alg, key });
        self.request_key_cache.insert(cache_key, Arc::clone(&key));
        Ok(Some(key))
    }

    async fn request_key_material(
        &self,
        row: &TenantRequestKeyRow,
        alg: AlgorithmId,
    ) -> Result<Vec<u8>, ProxyError> {
        let invalid = || ProxyError::Forbidden("Invalid request binding key".to_string());
        let materials = self.root_materials().await?;
        let (master_key, integrity_key) = decode_root_materials(&materials)?;
        let tenant = row.tenant_id.to_string();
        let (material, scope) = if alg.kind() == AlgorithmKind::Mac {
            let wrapped = row.encrypted_material.as_deref().ok_or_else(invalid)?;
            let key = unwrap_tenant_key(wrapped, &tenant, &row.kid, &master_key)
                .map_err(|_| invalid())?;
            (key, row.kid.clone())
        } else {
            let public = row.public_key.as_deref().ok_or_else(invalid)?;
            (public.as_bytes().to_vec(), public_key_digest_scope(&row.kid))
        };
        let digest_ok = verify_secret_key_digest(
            &integrity_key,
            &tenant,
            &scope,
            &material,
            &row.material_digest,
        )
        .map_err(|_| invalid())?;
        if !digest_ok {
            return Err(invalid());
        }
        Ok(material)
    }
}

/// 已校验、已解析的请求绑定密钥。
#[derive(Debug, Clone)]
pub struct RequestBindingKey {
    /// 密钥算法（绑定闭集内）。
    pub alg: AlgorithmId,
    /// HMAC 原始密钥或客户端公钥。
    pub key: KeyRef,
}

#[derive(Debug, Clone, Deserialize, sqlx::FromRow)]
struct TenantRequestKeyRow {
    pub tenant_id: Uuid,
    pub kid: String,
    pub alg: String,
    pub encrypted_material: Option<String>,
    pub public_key: Option<String>,
    pub material_digest: String,
    pub status: String,
    pub not_before: Option<chrono::DateTime<Utc>>,
    pub not_after: Option<chrono::DateTime<Utc>>,
}

impl TenantRequestKeyRow {
    fn usable_at(&self, now: chrono::DateTime<Utc>) -> bool {
        matches!(self.status.as_str(), "active" | "retiring")
            && self.not_before.is_none_or(|not_before| now >= not_before)
            && self.not_after.is_none_or(|not_after| now < not_after)
    }
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
    let now = Utc::now().timestamp();
    let skew = i64::from(clock_skew_seconds);
    if (now - envelope.timestamp).abs() > skew {
        return Err(ProxyError::Forbidden(
            "Encrypted request timestamp outside allowed clock skew".to_string(),
        ));
    }
    Ok(())
}

fn parse_transport_mode(raw: &str) -> Result<TransportMode, ProxyError> {
    match raw {
        "none" => Ok(TransportMode::None),
        "request" => Ok(TransportMode::Request),
        "full" => Ok(TransportMode::Full),
        other => Err(ProxyError::BadRequest(format!(
            "unsupported encryption_mode '{other}' (expected none|request|full)"
        ))),
    }
}

fn parse_optional_alg(raw: Option<&str>, field: &str) -> Result<Option<AlgorithmId>, ProxyError> {
    match raw {
        None => Ok(None),
        Some(value) => parse_algorithm_id(value).map(Some).map_err(|error| match error {
            CryptoError::UnknownAlgorithmId { raw } => ProxyError::BadRequest(format!(
                "unsupported {field} '{raw}' (legacy uppercase/aliases are denied; use kebab-case closed-set id)"
            )),
            other => ProxyError::BadRequest(format!("invalid {field}: {other}")),
        }),
    }
}

fn map_crypto_profile_error(error: CryptoError) -> ProxyError {
    match error {
        CryptoError::LegacyNotAllowed { id } => ProxyError::Forbidden(format!(
            "legacy algorithm denied (allow_insecure=false): {id}"
        )),
        CryptoError::UnknownAlgorithmId { raw } => ProxyError::BadRequest(format!(
            "unsupported algorithm '{raw}' (closed-set kebab-case required)"
        )),
        CryptoError::NotRegistered { id } => ProxyError::BadRequest(format!(
            "unsupported algorithm '{id}' (not registered)"
        )),
        CryptoError::UnknownSuite { name } => ProxyError::BadRequest(format!(
            "unsupported suite '{name}'"
        )),
        CryptoError::InvalidProfile { reason } => ProxyError::BadRequest(format!(
            "invalid tenant transport profile: {reason}"
        )),
        other => ProxyError::BadRequest(format!("invalid tenant transport profile: {other}")),
    }
}

fn map_transport_crypto_error(
    error: mazbri_core::crypto::transport::TransportCryptoError,
) -> ProxyError {
    use mazbri_core::crypto::transport::TransportCryptoError;
    match error {
        TransportCryptoError::UnsupportedAlgorithm => ProxyError::BadRequest(
            "unsupported transport algorithm (closed-set kebab-case required; legacy aliases denied)"
                .to_string(),
        ),
        TransportCryptoError::Crypto(inner) => map_crypto_profile_error(inner),
        TransportCryptoError::InvalidProfile(reason) => ProxyError::BadRequest(format!(
            "invalid transport envelope/profile: {reason}"
        )),
        TransportCryptoError::SignatureMismatch => {
            ProxyError::Forbidden("Invalid encrypted request signature".to_string())
        }
        TransportCryptoError::DecryptFailed | TransportCryptoError::InvalidCiphertext => {
            ProxyError::BadRequest("Invalid encrypted request body".to_string())
        }
        TransportCryptoError::InvalidKeyLength | TransportCryptoError::InvalidIv => {
            ProxyError::BadRequest("Invalid encrypted request envelope".to_string())
        }
        TransportCryptoError::InvalidSignature => {
            ProxyError::BadRequest("Invalid encrypted request signature encoding".to_string())
        }
        TransportCryptoError::EncryptFailed => {
            ProxyError::InternalError("failed to encrypt transport envelope".to_string())
        }
    }
}

/// Suggested Retry-After when the idempotency store is unavailable.
pub const IDEMPOTENCY_UNAVAILABLE_RETRY_AFTER_SECS: u64 = 1;

/// Fail-closed: a keyed request must not run without the idempotency guard
/// (would risk double charge). Logs, bumps the metric, returns 503 error.
pub(crate) fn idempotency_unavailable(
    op: &str,
    fingerprint: &str,
    error: &dyn std::fmt::Display,
) -> ProxyError {
    crate::gateway::metrics::GATEWAY_IDEMPOTENCY_UNAVAILABLE_TOTAL.inc();
    tracing::error!(
        target: "gateway.stage",
        event = "idempotency_unavailable",
        op = op,
        fingerprint = %fingerprint,
        error = %error,
        "Idempotency store failed; rejecting keyed request (fail-closed)"
    );
    ProxyError::IdempotencyUnavailable {
        retry_after_secs: IDEMPOTENCY_UNAVAILABLE_RETRY_AFTER_SECS,
    }
}

#[cfg(test)]
mod transport_profile_tests {
    use super::{
        map_crypto_profile_error, map_transport_crypto_error, parse_optional_alg,
        parse_transport_mode, TenantCryptoProfile,
    };
    use crate::gateway::ProxyError;
    use mazbri_core::crypto::transport::{
        SealEnvelopeParams, open_envelope, seal_envelope, TransportCryptoError,
    };
    use mazbri_core::crypto::{
        AlgorithmId, AlgorithmRegistry, CryptoError, KeySelection, TransportMode,
        SUITE_TENANT_DEFAULT_2026_11,
    };

    fn full_aes_db_profile() -> TenantCryptoProfile {
        TenantCryptoProfile {
            encryption_mode: "full".into(),
            profile_version: 1,
            suite: Some(SUITE_TENANT_DEFAULT_2026_11.into()),
            data_alg: Some("aes-256-gcm".into()),
            mac_alg: None,
            require_signature: false,
            allow_insecure: false,
            require_request_binding: false,
            signature_version: "hmac-v1".into(),
            current_key_version: Some("v1".into()),
            nonce_ttl_seconds: 300,
            clock_skew_seconds: 300,
        }
    }

    #[test]
    fn parses_modes_and_rejects_unknown() {
        assert!(matches!(
            parse_transport_mode("none").unwrap(),
            TransportMode::None
        ));
        assert!(matches!(
            parse_transport_mode("request").unwrap(),
            TransportMode::Request
        ));
        assert!(matches!(
            parse_transport_mode("full").unwrap(),
            TransportMode::Full
        ));
        let err = parse_transport_mode("legacy").unwrap_err();
        assert!(err.to_string().contains("unsupported encryption_mode"));
    }

    #[test]
    fn rejects_uppercase_algorithm_alias() {
        let err = parse_optional_alg(Some("AES-256-GCM"), "data_alg").unwrap_err();
        assert!(err.to_string().contains("unsupported data_alg"));
    }

    #[test]
    fn mode_full_aes_roundtrip_via_profile_seal_open() {
        let registry = AlgorithmRegistry::default_registry();
        let db_profile = full_aes_db_profile();
        let transport = db_profile.transport_profile().expect("transport");
        transport.validate(&registry).expect("validate");
        let key = [11u8; 32];
        let plaintext = br#"{"ok":true}"#;
        let sealed = seal_envelope(SealEnvelopeParams {
            profile: &transport,
            registry: &registry,
            data_key: &key,
            mac_key: None,
            key_version: "v1",
            tenant_id: "11111111-1111-1111-1111-111111111111",
            http_path: "/api/v1/gateway",
            plaintext,
            timestamp: 1_710_000_000,
        })
        .expect("seal");
        assert_eq!(sealed.data_alg, "aes-256-gcm");
        let opened = open_envelope(
            &transport,
            &registry,
            &key,
            None,
            "11111111-1111-1111-1111-111111111111",
            "/api/v1/gateway",
            &sealed,
        )
        .expect("open");
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn mode_none_cannot_be_used_for_encrypting_profile_fields() {
        let profile = TenantCryptoProfile {
            encryption_mode: "none".into(),
            profile_version: 1,
            suite: None,
            data_alg: None,
            mac_alg: None,
            require_signature: false,
            allow_insecure: false,
            require_request_binding: false,
            signature_version: "hmac-v1".into(),
            current_key_version: None,
            nonce_ttl_seconds: 300,
            clock_skew_seconds: 300,
        };
        let transport = profile.transport_profile().expect("transport");
        assert!(matches!(transport.mode, TransportMode::None));
        let registry = AlgorithmRegistry::default_registry();
        transport.validate(&registry).expect("plaintext profile ok");
    }

    #[test]
    fn legacy_denied_maps_to_forbidden() {
        let err = map_crypto_profile_error(CryptoError::LegacyNotAllowed {
            id: AlgorithmId::Aes256Ecb,
        });
        match err {
            ProxyError::Forbidden(msg) => {
                assert!(msg.contains("legacy algorithm denied"));
                assert!(msg.contains("aes-256-ecb"));
            }
            other => panic!("unexpected {other:?}"),
        }
        let err = map_transport_crypto_error(TransportCryptoError::UnsupportedAlgorithm);
        assert!(matches!(err, ProxyError::BadRequest(_)));
    }

    #[test]
    fn key_selection_uses_current_key_version() {
        let transport = full_aes_db_profile().transport_profile().unwrap();
        match transport.key {
            Some(KeySelection::Latest { key_id }) => assert_eq!(key_id, "v1"),
            other => panic!("unexpected key {other:?}"),
        }
    }
}

#[cfg(test)]
mod idempotency_fail_closed_tests {
    use super::idempotency_unavailable;
    use crate::gateway::metrics::GATEWAY_IDEMPOTENCY_UNAVAILABLE_TOTAL;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[tokio::test]
    async fn redis_failure_maps_to_503_with_retry_after_and_code() {
        let before = GATEWAY_IDEMPOTENCY_UNAVAILABLE_TOTAL.get();
        let err = idempotency_unavailable("SET NX", "fp-1", &"connection refused");
        assert!(GATEWAY_IDEMPOTENCY_UNAVAILABLE_TOTAL.get() > before);
        assert_eq!(err.error_code(), "IDEMPOTENCY_UNAVAILABLE");
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()),
            Some("1")
        );
        let body = axum::body::to_bytes(response.into_body(), 1 << 16)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");
        assert!(text.contains("IDEMPOTENCY_UNAVAILABLE"));
    }
}
