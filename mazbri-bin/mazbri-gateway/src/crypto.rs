use crate::gateway::ProxyError;
use crate::gateway::validation::GatewayRequest;
use crate::gateway::validation::GatewayRequestPayload;
use crate::redis_client::RedisClient;
use chrono::Utc;
use mazbri_core::crypto::supplier_secrets::{
    RootMaterialKeyring, install_root_material_keyring,
};
use mazbri_core::crypto::tenant_key::{
    fetch_root_protection_keyring, unwrap_and_verify_tenant_key_with_keyring,
};
use mazbri_core::crypto::transport::{
    EnvelopeDirection, SealEnvelopeParams, TransportEnvelope, open_envelope, seal_envelope,
};
use mazbri_core::crypto::{
    AlgorithmId, AlgorithmRegistry, CryptoError, TenantTransportProfile, TransportMode,
    parse_algorithm_id,
};
use mazbri_core::metrics::{MetricsBuilder, REGISTRY, register_collector_once};
use prometheus::{CounterVec, HistogramOpts, HistogramVec};
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};
use uuid::Uuid;
use zeroize::Zeroizing;

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
    pub suite: Option<String>,
    pub data_alg: Option<String>,
    pub mac_alg: Option<String>,
    pub require_signature: bool,
    pub allow_insecure: bool,
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
        let nonce_ttl_secs = u32::try_from(self.nonce_ttl_seconds).map_err(|_| {
            ProxyError::InternalError("tenant crypto nonce_ttl_seconds must be >= 0".to_string())
        })?;
        let clock_skew_secs = u32::try_from(self.clock_skew_seconds).map_err(|_| {
            ProxyError::InternalError("tenant crypto clock_skew_seconds must be >= 0".to_string())
        })?;
        Ok(TenantTransportProfile {
            mode,
            suite: self.suite.clone(),
            data_alg,
            mac_alg,
            require_signature: self.require_signature,
            allow_insecure: self.allow_insecure,
            nonce_ttl_secs,
            clock_skew_secs,
        })
    }
}

#[derive(Debug, Clone, Deserialize, sqlx::FromRow)]
pub struct TenantCryptoKey {
    pub tenant_id: Uuid,
    pub encrypted_secret_key: String,
    pub secret_key_digest: String,
}

#[derive(Debug, Clone, Deserialize, sqlx::FromRow)]
struct TenantCryptoSnapshotRow {
    pub tenant_id: Uuid,
    pub encryption_mode: String,
    pub suite: Option<String>,
    pub data_alg: Option<String>,
    pub mac_alg: Option<String>,
    pub require_signature: bool,
    pub allow_insecure: bool,
    pub nonce_ttl_seconds: i32,
    pub clock_skew_seconds: i32,
    pub encrypted_secret_key: Option<String>,
    pub secret_key_digest: Option<String>,
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
    key_cache: moka::sync::Cache<Uuid, Arc<Zeroizing<Vec<u8>>>>,
    root_material_cache: moka::sync::Cache<String, Arc<RootMaterialKeyring>>,
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
                suite: None,
                data_alg: None,
                mac_alg: None,
                require_signature: false,
                allow_insecure: false,
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
                tenant_id = %tenant_id,
                error = %error,
                "Tenant crypto key warmup failed"
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
        let sm2_started_at = Instant::now();
        match tokio::task::spawn_blocking(mazbri_core::crypto::warm_sm2).await {
            Ok(warm_elapsed) => {
                record_crypto_maintenance("sm2_table_warmup", "success", sm2_started_at.elapsed());
                info!(
                    elapsed_ms = warm_elapsed.as_millis(),
                    wall_ms = sm2_started_at.elapsed().as_millis(),
                    "SM2 precompute table (g_table) warmed before ready"
                );
            }
            Err(error) => {
                record_crypto_maintenance("sm2_table_warmup", "error", sm2_started_at.elapsed());
                warn!(error = %error, "SM2 precompute table warmup task failed");
                last_error = Some(ProxyError::InternalError(format!(
                    "SM2 warm_sm2 spawn_blocking join failed: {error}"
                )));
            }
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
            SELECT p.tenant_id, p.encryption_mode, p.suite, p.data_alg, p.mac_alg,
                p.require_signature, p.allow_insecure,
                p.nonce_ttl_seconds, p.clock_skew_seconds,
                k.encrypted_secret_key, k.secret_key_digest
            FROM tenant_crypto_profiles p
            LEFT JOIN tenant_crypto_keys k
                ON k.tenant_id = p.tenant_id
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
        let root_keyring = match self.root_materials().await {
            Ok(keyring) => Some(keyring),
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
                if let Some(keyring) = root_keyring.as_deref() {
                    match Self::materialize_key_row(&key_row, keyring) {
                        Ok(key) => {
                            self.key_cache.insert(key_row.tenant_id, key);
                        }
                        Err(error) => {
                            warn!(
                                tenant_id = %key_row.tenant_id,
                                error = %error,
                                "Tenant crypto key refresh failed"
                            );
                        }
                    }
                } else {
                    warn!(
                        tenant_id = %snapshot.tenant_id,
                        "Tenant crypto key refresh skipped because root material cache was unavailable"
                    );
                }
            } else if matches!(snapshot.encryption_mode.as_str(), "request" | "full") {
                warn!(
                    tenant_id = %snapshot.tenant_id,
                    "Tenant crypto key row was not found during snapshot refresh"
                );
            }
        }
        self.evict_stale_crypto_caches(active_tenants).await
    }

    async fn evict_stale_crypto_caches(
        &self,
        active_tenants: HashSet<Uuid>,
    ) -> Result<(), ProxyError> {
        let usable_key_tenants = sqlx::query_scalar::<_, Uuid>(
            r"
            SELECT k.tenant_id
            FROM tenant_crypto_keys k
            JOIN tenant_crypto_profiles p ON p.tenant_id = k.tenant_id
            WHERE p.encryption_mode <> 'none'
            ",
        )
        .fetch_all(self.db.as_ref())
        .await
        .map_err(|error| {
            ProxyError::InternalError(format!("usable crypto keys query failed: {error}"))
        })?;
        let usable_cache_keys: Arc<HashSet<Uuid>> =
            Arc::new(usable_key_tenants.into_iter().collect());
        if let Err(error) = self.key_cache.invalidate_entries_if({
            let usable_cache_keys = Arc::clone(&usable_cache_keys);
            move |tenant_id, _| !usable_cache_keys.contains(tenant_id)
        }) {
            warn!(
                error = %error,
                "Tenant crypto key cache invalidation closure is unavailable"
            );
        }

        let active_tenants = Arc::new(active_tenants);
        let invalidate_result = self.profile_cache.invalidate_entries_if({
            let active_tenants = Arc::clone(&active_tenants);
            move |tenant_id, _| !active_tenants.contains(tenant_id)
        });
        if let Err(error) = invalidate_result {
            warn!(
                error = %error,
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
        let keyring = self.fetch_and_install_keyring().await?;
        self.root_material_cache.insert(cache_key, keyring);
        Ok(())
    }

    async fn load_profile_snapshot(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<TenantCryptoSnapshotRow>, ProxyError> {
        sqlx::query_as::<_, TenantCryptoSnapshotRow>(
            r"
            SELECT p.tenant_id, p.encryption_mode, p.suite, p.data_alg, p.mac_alg,
                p.require_signature, p.allow_insecure,
                p.nonce_ttl_seconds, p.clock_skew_seconds,
                k.encrypted_secret_key, k.secret_key_digest
            FROM tenant_crypto_profiles p
            LEFT JOIN tenant_crypto_keys k
                ON k.tenant_id = p.tenant_id
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
            suite: snapshot.suite.clone(),
            data_alg: snapshot.data_alg.clone(),
            mac_alg: snapshot.mac_alg.clone(),
            require_signature: snapshot.require_signature,
            allow_insecure: snapshot.allow_insecure,
            nonce_ttl_seconds: snapshot.nonce_ttl_seconds,
            clock_skew_seconds: snapshot.clock_skew_seconds,
        }
    }

    fn key_from_snapshot(snapshot: &TenantCryptoSnapshotRow) -> Option<TenantCryptoKey> {
        Some(TenantCryptoKey {
            tenant_id: snapshot.tenant_id,
            encrypted_secret_key: snapshot.encrypted_secret_key.clone()?,
            secret_key_digest: snapshot.secret_key_digest.clone()?,
        })
    }

    fn materialize_key_row(
        row: &TenantCryptoKey,
        keyring: &RootMaterialKeyring,
    ) -> Result<Arc<Zeroizing<Vec<u8>>>, ProxyError> {
        let key = unwrap_and_verify_tenant_key_with_keyring(
            &row.encrypted_secret_key,
            &row.tenant_id.to_string(),
            &row.secret_key_digest,
            keyring,
        )
        .map_err(|_| ProxyError::Forbidden("Invalid encrypted request".to_string()))?;
        Ok(Arc::new(key))
    }

    async fn materialize_key_row_with_root(
        &self,
        row: &TenantCryptoKey,
    ) -> Result<Arc<Zeroizing<Vec<u8>>>, ProxyError> {
        let keyring = self.root_materials().await?;
        if let Ok(key) = Self::materialize_key_row(row, &keyring) {
            self.key_cache.insert(row.tenant_id, Arc::clone(&key));
            return Ok(key);
        }
        self.refresh_root_material_cache().await?;
        let refreshed = self.root_materials().await?;
        let key = Self::materialize_key_row(row, &refreshed)?;
        self.key_cache.insert(row.tenant_id, Arc::clone(&key));
        Ok(key)
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
                let tenant_key = self.tenant_key(tenant_id, &mut timing).await?;
                let signature_started_at = Instant::now();
                let decrypt_started_at = Instant::now();
                let plaintext = open_envelope(
                    &transport,
                    AlgorithmRegistry::global(),
                    tenant_key.as_slice(),
                    EnvelopeDirection::ClientToGateway,
                    &tenant_id_str,
                    http_path,
                    &envelope,
                )
                .map_err(map_transport_crypto_error)?;
                timing.signature = mazbri_core::num::millis_u64(signature_started_at.elapsed());
                timing.decrypt = mazbri_core::num::millis_u64(decrypt_started_at.elapsed());
                self.check_nonce(tenant_id, &envelope, profile.nonce_ttl_seconds, &mut timing)
                    .await?;
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
        let transport = profile.transport_profile()?;
        let mut timing = TenantCryptoTiming::default();
        let tenant_key = self.tenant_key(tenant_id, &mut timing).await?;
        let tenant_id_str = tenant_id.to_string();
        seal_envelope(SealEnvelopeParams {
            profile: &transport,
            registry: AlgorithmRegistry::global(),
            master_key: tenant_key.as_slice(),
            direction: EnvelopeDirection::GatewayToClient,
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
        timing: &mut TenantCryptoTiming,
    ) -> Result<Arc<Zeroizing<Vec<u8>>>, ProxyError> {
        let started_at = Instant::now();
        if let Some(key) = self.key_cache.get(&tenant_id) {
            record_crypto_cache_result("key", "hit");
            record_crypto_lookup_duration("key", "hit", started_at.elapsed());
            timing.tenant_key = mazbri_core::num::millis_u64(started_at.elapsed());
            return Ok(key);
        }
        record_crypto_cache_result("key", "miss");
        let db_started_at = Instant::now();
        let row = sqlx::query_as::<_, TenantCryptoKey>(
            r"
            SELECT tenant_id, encrypted_secret_key, secret_key_digest
            FROM tenant_crypto_keys
            WHERE tenant_id = $1
            ",
        )
        .bind(tenant_id)
        .fetch_optional(self.db.as_ref())
        .await;
        timing.tenant_key_db = mazbri_core::num::millis_u64(db_started_at.elapsed());
        let row = row
            .map_err(|error| {
                ProxyError::InternalError(format!("crypto key lookup failed: {error}"))
            })?
            .ok_or_else(|| ProxyError::Forbidden("Invalid encrypted request".to_string()))?;
        let root_material_started_at = Instant::now();
        let mut keyring = self.root_materials().await?;
        timing.tenant_key_root_material =
            mazbri_core::num::millis_u64(root_material_started_at.elapsed());
        let local_started_at = Instant::now();
        let key = if let Ok(materialized) = Self::materialize_key_row(&row, &keyring) {
            materialized
        } else {
            self.refresh_root_material_cache().await?;
            keyring = self.root_materials().await?;
            Self::materialize_key_row(&row, &keyring)?
        };
        timing.tenant_key_local = mazbri_core::num::millis_u64(local_started_at.elapsed());
        timing.tenant_key = mazbri_core::num::millis_u64(started_at.elapsed());
        self.key_cache.insert(tenant_id, Arc::clone(&key));
        record_crypto_lookup_duration("key", "miss", started_at.elapsed());
        Ok(key)
    }

    async fn fetch_and_install_keyring(&self) -> Result<Arc<RootMaterialKeyring>, ProxyError> {
        let protection = fetch_root_protection_keyring(
            &self.http_client,
            &self.cert_service_url,
            &self.cert_service_token,
        )
        .await
        .map_err(|error| {
            ProxyError::InternalError(format!("tenant crypto root keyring fetch failed: {error}"))
        })?;
        let keyring =
            RootMaterialKeyring::from_protection_keyring(&protection).map_err(|error| {
                ProxyError::InternalError(format!(
                    "tenant crypto root keyring decode failed: {error}"
                ))
            })?;
        install_root_material_keyring(keyring.clone());
        Ok(Arc::new(keyring))
    }

    async fn root_materials(&self) -> Result<Arc<RootMaterialKeyring>, ProxyError> {
        let cache_key = "active".to_string();
        if let Some(keyring) = self.root_material_cache.get(&cache_key) {
            return Ok(keyring);
        }
        let _guard = self.root_material_refresh_lock.lock().await;
        if let Some(keyring) = self.root_material_cache.get(&cache_key) {
            return Ok(keyring);
        }
        let keyring = self.fetch_and_install_keyring().await?;
        self.root_material_cache
            .insert(cache_key, Arc::clone(&keyring));
        Ok(keyring)
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
                .set_nonce_if_absent(tenant_id, &envelope.nonce, ttl_seconds)
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

#[cfg(test)]
mod transport_profile_tests {
    use super::{
        TenantCryptoProfile, map_crypto_profile_error, map_transport_crypto_error,
        parse_optional_alg, parse_transport_mode,
    };
    use crate::gateway::ProxyError;
    use mazbri_core::crypto::transport::{
        EnvelopeDirection, SealEnvelopeParams, TransportCryptoError, open_envelope, seal_envelope,
    };
    use mazbri_core::crypto::{
        AlgorithmId, AlgorithmRegistry, CryptoError, SUITE_AES_256_GCM, TransportMode,
    };

    fn full_aes_db_profile() -> TenantCryptoProfile {
        TenantCryptoProfile {
            encryption_mode: "full".into(),
            suite: Some(SUITE_AES_256_GCM.into()),
            data_alg: Some("aes-256-gcm".into()),
            mac_alg: None,
            require_signature: false,
            allow_insecure: false,
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
        let registry = AlgorithmRegistry::global();
        let db_profile = full_aes_db_profile();
        let transport = db_profile.transport_profile().expect("transport");
        transport.validate(registry).expect("validate");
        let key = [11u8; 32];
        let plaintext = br#"{"ok":true}"#;
        let sealed = seal_envelope(SealEnvelopeParams {
            profile: &transport,
            registry,
            master_key: &key,
            direction: EnvelopeDirection::ClientToGateway,
            tenant_id: "11111111-1111-1111-1111-111111111111",
            http_path: "/api/v1/gateway",
            plaintext,
            timestamp: 1_710_000_000,
        })
        .expect("seal");
        assert_eq!(sealed.data_alg, "aes-256-gcm");
        let opened = open_envelope(
            &transport,
            registry,
            &key,
            EnvelopeDirection::ClientToGateway,
            "11111111-1111-1111-1111-111111111111",
            "/api/v1/gateway",
            &sealed,
        )
        .expect("open");
        assert_eq!(opened, plaintext);
        assert!(
            open_envelope(
                &transport,
                registry,
                &key,
                EnvelopeDirection::GatewayToClient,
                "11111111-1111-1111-1111-111111111111",
                "/api/v1/gateway",
                &sealed,
            )
            .is_err()
        );
    }

    #[test]
    fn mode_none_cannot_be_used_for_encrypting_profile_fields() {
        let profile = TenantCryptoProfile {
            encryption_mode: "none".into(),
            suite: None,
            data_alg: None,
            mac_alg: None,
            require_signature: false,
            allow_insecure: false,
            nonce_ttl_seconds: 300,
            clock_skew_seconds: 300,
        };
        let transport = profile.transport_profile().expect("transport");
        assert!(matches!(transport.mode, TransportMode::None));
        let registry = AlgorithmRegistry::global();
        transport.validate(registry).expect("plaintext profile ok");
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
}
