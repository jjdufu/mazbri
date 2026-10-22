use super::{AuthResult, AuthServiceConfig, error::AuthError};
use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use uuid::Uuid;

const MAX_BUFFER_SIZE: usize = 10_000;
/// Cap validation-cache entries so RSS cannot grow unbounded with unique keys.
const DEFAULT_MAX_VALIDATION_CACHE_ENTRIES: usize = 10_000;

/// Build a cache key that does **not** store the raw API key secret.
/// Format: `{prefix}:{sha256_hex(full_key)}` — stable, fixed-size secret material.
fn validation_cache_key(api_key: &str, prefix: &str) -> String {
    let digest = Sha256::digest(api_key.as_bytes());
    format!("{prefix}:{}", hex::encode(digest))
}

pub struct LastUsedBatchWriter {
    buffer: Arc<DashMap<Uuid, std::time::SystemTime>>,
    db: Arc<PgPool>,
    cancel_token: CancellationToken,
    /// Join handle is set once at construction and taken once at shutdown.
    /// `std::sync::Mutex` with **instantaneous** critical sections only (no `.await`
    /// while held). Not on the validate hot path.
    flush_task_handle: Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl Clone for LastUsedBatchWriter {
    fn clone(&self) -> Self {
        Self {
            buffer: Arc::clone(&self.buffer),
            db: Arc::clone(&self.db),
            cancel_token: self.cancel_token.clone(),
            flush_task_handle: Arc::clone(&self.flush_task_handle),
        }
    }
}

impl LastUsedBatchWriter {
    pub fn new(db: Arc<PgPool>) -> Self {
        let cancel_token = CancellationToken::new();
        let buffer = Arc::new(DashMap::new());
        let buffer_task = Arc::clone(&buffer);
        let db_task = Arc::clone(&db);
        let cancel = cancel_token.clone();
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tokio::select! {
                    _ = interval.tick() => { Self::flush(&buffer_task, &db_task).await; }
                    () = cancel.cancelled() => {
                        debug!("LastUsedBatchWriter shutdown signal received, performing final flush");
                        Self::flush(&buffer_task, &db_task).await;
                        break;
                    }
                }
            }
        });
        Self {
            buffer,
            db,
            cancel_token,
            flush_task_handle: Arc::new(std::sync::Mutex::new(Some(handle))),
        }
    }

    pub fn record(&self, credential_id: Uuid) {
        if self.buffer.len() >= MAX_BUFFER_SIZE {
            crate::metrics::API_KEY_LAST_USED_DROPPED_TOTAL.inc();
            warn!(
                buffer_size = self.buffer.len(),
                "LastUsedBatchWriter buffer at capacity, dropping record"
            );
            return;
        }
        self.buffer
            .insert(credential_id, std::time::SystemTime::now());
    }

    pub async fn shutdown(&self) {
        self.cancel_token.cancel();
        let handle = {
            // Instant critical section: take handle, drop guard, then await.
            let mut guard = self
                .flush_task_handle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.take()
        };
        if let Some(h) = handle {
            let _ = h.await;
        }
    }

    async fn flush(buffer: &DashMap<Uuid, std::time::SystemTime>, db: &PgPool) {
        let mut ids = HashSet::new();
        for entry in buffer {
            ids.insert(*entry.key());
        }
        if ids.is_empty() {
            return;
        }
        let ids: Vec<Uuid> = ids.into_iter().collect();
        match sqlx::query("UPDATE auth_credentials SET last_used_at = NOW() WHERE id = ANY($1)")
            .bind(&ids)
            .execute(db)
            .await
        {
            Ok(result) => {
                debug!(
                    updated = % result.rows_affected(), buffered = ids.len(),
                    "Flushed last_used_at batch"
                );
                for id in &ids {
                    buffer.remove(id);
                }
            }
            Err(e) => {
                warn!(
                    error = % e, entry_count = ids.len(),
                    "Failed to flush last_used_at batch, entries retained for retry"
                );
            }
        }
    }
}

#[derive(Clone)]
pub struct ApiKeyValidator {
    db: Arc<PgPool>,
    config: Arc<AuthServiceConfig>,
    last_used_writer: LastUsedBatchWriter,
    /// Keys are `prefix:sha256(full_key)` — never the raw secret.
    validation_cache: Arc<DashMap<String, CachedApiKeyAuth>>,
}

#[derive(Debug, Clone)]
struct CachedApiKeyAuth {
    auth: ApiKeyAuth,
    expires_at: Instant,
    /// API key prefix for prefix-scoped invalidation without scanning full secrets.
    prefix: String,
}

impl ApiKeyValidator {
    #[must_use]
    pub fn new(db: Arc<PgPool>, config: Arc<AuthServiceConfig>) -> Self {
        let last_used_writer = LastUsedBatchWriter::new(db.clone());
        Self {
            db,
            config,
            last_used_writer,
            validation_cache: Arc::new(DashMap::new()),
        }
    }

    fn max_cache_entries(&self) -> usize {
        let n = self.config.validation_cache_max_entries;
        if n == 0 {
            DEFAULT_MAX_VALIDATION_CACHE_ENTRIES
        } else {
            n
        }
    }

    fn insert_cache(&self, cache_key: String, entry: CachedApiKeyAuth) {
        let max = self.max_cache_entries();
        if self.validation_cache.len() >= max {
            // Prefer dropping expired entries; if still full, skip caching this hit.
            let now = Instant::now();
            let expired: Vec<String> = self
                .validation_cache
                .iter()
                .filter(|e| e.expires_at <= now)
                .map(|e| e.key().clone())
                .take(64)
                .collect();
            for k in expired {
                self.validation_cache.remove(&k);
            }
            if self.validation_cache.len() >= max {
                debug!(
                    cache_len = self.validation_cache.len(),
                    max, "API key validation cache at capacity; skipping insert"
                );
                return;
            }
        }
        self.validation_cache.insert(cache_key, entry);
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn validate(&self, api_key: &str) -> AuthResult<ApiKeyAuth> {
        let started_at = std::time::Instant::now();
        let parts: Vec<&str> = api_key.split('_').collect();
        if parts.len() != 3 || parts[0] != "mazbri" {
            return Err(AuthError::InvalidApiKeyFormat(
                "Invalid API key format".to_string(),
            ));
        }
        let prefix = parts[1];
        let hash = parts[2];
        let cache_key = validation_cache_key(api_key, prefix);

        if let Some(auth) = self.cached_validation(&cache_key, prefix, started_at) {
            return Ok(auth);
        }
        let row_opt = sqlx::query(
            r"
            SELECT
                ac.id,
                ac.tenant_id,
                ac.credential_secret_hash,
                ac.is_active,
                ak.is_active AS api_key_active,
                ac.expires_at,
                COALESCE(ak.scopes, ARRAY[]::TEXT[]) AS scopes
            FROM auth_credentials ac
            LEFT JOIN api_keys ak ON ak.credential_id = ac.id
            WHERE ac.credential_type = 'api_key'
                AND ac.credential_id = $1
            ",
        )
        .bind(prefix)
        .fetch_optional(&*self.db)
        .await?;

        if let Some(row) = row_opt {
            self.authenticate_row(&row, prefix, hash, cache_key, started_at)
                .await
        } else {
            Err(reject_unknown_prefix(prefix, hash).await)
        }
    }

    fn cached_validation(
        &self,
        cache_key: &str,
        prefix: &str,
        started_at: std::time::Instant,
    ) -> Option<ApiKeyAuth> {
        if let Some(entry) = self.validation_cache.get(cache_key) {
            if entry.expires_at > Instant::now() {
                let auth = entry.auth.clone();
                drop(entry);
                self.last_used_writer.record(auth.credential_id);
                debug!(
                    prefix = % prefix, tenant_id = % auth.tenant_id, total_elapsed_ms =
                    started_at.elapsed().as_millis(), "API key validation cache hit"
                );
                return Some(auth);
            }
            drop(entry);
            self.validation_cache.remove(cache_key);
        }

        None
    }

    async fn authenticate_row(
        &self,
        row: &sqlx::postgres::PgRow,
        prefix: &str,
        hash: &str,
        cache_key: String,
        started_at: std::time::Instant,
    ) -> AuthResult<ApiKeyAuth> {
        debug!(
            prefix = % prefix, elapsed_ms = started_at.elapsed().as_millis(),
            "API key metadata loaded from database"
        );
        let is_active: bool = row.try_get("is_active")?;
        let api_key_active: Option<bool> = row.try_get("api_key_active")?;
        if !is_active || api_key_active == Some(false) {
            return Err(AuthError::ApiKeyDisabled(prefix.to_string()));
        }
        let expires_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("expires_at")?;
        if let Some(exp) = expires_at
            && exp < chrono::Utc::now()
        {
            return Err(AuthError::ApiKeyExpired(prefix.to_string()));
        }
        let credential_secret_hash: String = row.try_get("credential_secret_hash")?;
        let hash_bytes = hash.as_bytes().to_vec();
        let verify_started_at = std::time::Instant::now();
        let verify_result = tokio::task::spawn_blocking(move || {
            crate::constant_time_argon2_verify(&hash_bytes, &credential_secret_hash)
        })
        .await
        .map_err(|e| {
            AuthError::Core(mazbri_core::CoreError::Internal(format!(
                "API key verify task failed: {e}"
            )))
        })?;
        debug!(
            prefix = % prefix, verify_elapsed_ms = verify_started_at.elapsed()
            .as_millis(), total_elapsed_ms = started_at.elapsed().as_millis(),
            "API key secret verification completed"
        );
        if verify_result.is_err() {
            return Err(AuthError::InvalidApiKeyFormat(
                "Invalid API key".to_string(),
            ));
        }
        let credential_id: Uuid = row.try_get("id")?;
        let tenant_id: Uuid = row.try_get("tenant_id")?;
        let scopes: Vec<String> = row.try_get("scopes").unwrap_or_default();
        self.last_used_writer.record(credential_id);
        let auth = ApiKeyAuth {
            credential_id,
            tenant_id,
            api_key_prefix: prefix.to_string(),
            scopes,
        };
        self.insert_cache(
            cache_key,
            CachedApiKeyAuth {
                auth: auth.clone(),
                expires_at: Instant::now()
                    + Duration::from_secs(self.config.validation_cache_ttl_secs),
                prefix: prefix.to_string(),
            },
        );
        debug!(
            prefix = % prefix, tenant_id = % auth.tenant_id, total_elapsed_ms =
            started_at.elapsed().as_millis(), "API key validated successfully"
        );
        Ok(auth)
    }

    pub fn cache_validation(&self, api_key: &str, auth: ApiKeyAuth, ttl: Duration) {
        let prefix = auth.api_key_prefix.clone();
        let cache_key = validation_cache_key(api_key, &prefix);
        self.insert_cache(
            cache_key,
            CachedApiKeyAuth {
                auth,
                expires_at: Instant::now() + ttl,
                prefix,
            },
        );
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn create_api_key(
        &self,
        tenant_id: Uuid,
        description: Option<String>,
    ) -> AuthResult<String> {
        let mut tx = self.db.begin().await?;
        let prefix = self.generate_prefix(&mut tx).await?;
        let random_bytes: [u8; 32] = rand::random();
        let secret = hex::encode(random_bytes);
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::default()
            .hash_password(secret.as_bytes(), &salt)
            .map_err(|e| {
                AuthError::Core(mazbri_core::CoreError::Internal(format!(
                    "Failed to hash password: {e}"
                )))
            })?
            .to_string();
        let metadata = description.map(|d| serde_json::json!({ "description" : d }));
        sqlx::query(
                r"
            INSERT INTO auth_credentials (tenant_id, credential_type, credential_id, credential_secret_hash, metadata)
            VALUES ($1, 'api_key', $2, $3, $4)
            ",
            )
            .bind(tenant_id)
            .bind(&prefix)
            .bind(&password_hash)
            .bind(metadata)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(format!("mazbri_{prefix}_{secret}"))
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn revoke_api_key(&self, prefix: &str) -> AuthResult<()> {
        let mut tx = self.db.begin().await?;
        let result = sqlx::query(
                "UPDATE auth_credentials SET is_active = false WHERE credential_type = 'api_key' AND credential_id = $1",
            )
            .bind(prefix)
            .execute(&mut *tx)
            .await?;
        if result.rows_affected() == 0 {
            tx.rollback().await?;
            return Err(AuthError::ApiKeyNotFound(prefix.to_string()));
        }
        sqlx::query(
                "UPDATE api_keys SET is_active = false, is_revoked = true, revoked_at = NOW() WHERE prefix = $1",
            )
            .bind(prefix)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.invalidate_validation_cache(prefix);
        Ok(())
    }

    pub async fn shutdown(&self) {
        self.last_used_writer.shutdown().await;
    }

    async fn generate_prefix(&self, tx: &mut Transaction<'_, Postgres>) -> AuthResult<String> {
        const MAX_PREFIX_ATTEMPTS: usize = 32;
        let hex_length = self.config.api_key_prefix_length;
        let byte_count = hex_length.div_ceil(2);
        for _ in 0..MAX_PREFIX_ATTEMPTS {
            let random_bytes: Vec<u8> = (0..byte_count).map(|_| rand::random::<u8>()).collect();
            let hex_str = hex::encode(&random_bytes);
            let prefix = hex_str[..hex_length].to_string();
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM auth_credentials WHERE credential_id = $1)",
            )
            .bind(&prefix)
            .fetch_one(&mut **tx)
            .await?;
            if !exists {
                return Ok(prefix);
            }
        }
        Err(AuthError::Core(mazbri_core::CoreError::Internal(format!(
            "failed to generate unique API key prefix after {MAX_PREFIX_ATTEMPTS} attempts \
             (prefix_length={hex_length})"
        ))))
    }

    fn invalidate_validation_cache(&self, prefix: &str) {
        let keys = self
            .validation_cache
            .iter()
            .filter_map(|entry| {
                if entry.prefix == prefix || entry.key().starts_with(&format!("{prefix}:")) {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for key in keys {
            self.validation_cache.remove(&key);
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApiKeyAuth {
    pub credential_id: Uuid,
    pub tenant_id: Uuid,
    pub api_key_prefix: String,
    /// Scopes stored on `api_keys.scopes` (may be empty for legacy keys).
    pub scopes: Vec<String>,
}

async fn reject_unknown_prefix(prefix: &str, hash: &str) -> AuthError {
    const DUMMY_HASH: &str = "$argon2id$v=19$m=65536,t=3,p=4$pZTjh2UekhkaTDHbZbWhSQ$meFTW0nHJXRVXp7NIp0vNR7B4wY4chWoSdlYhtzYWdw";
    let hash_bytes = hash.as_bytes().to_vec();
    let _verify_result = tokio::task::spawn_blocking(move || {
        crate::constant_time_argon2_verify(&hash_bytes, DUMMY_HASH)
    })
    .await;
    AuthError::ApiKeyNotFound(prefix.to_string())
}
