use crate::num::SaturatingFrom;
use crate::{CircuitBreakerConfig, CircuitResult, CircuitState, CoreError};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
#[path = "distributed_circuit_breaker/state_machine.rs"]
mod state_machine;
#[path = "distributed_circuit_breaker/storage_sync.rs"]
mod storage_sync;
#[derive(Debug, Clone)]
pub(crate) struct LocalCacheEntry {
    state: CircuitState,
    last_state_change: Instant,
    cached_at: Instant,
}
impl Default for LocalCacheEntry {
    fn default() -> Self {
        Self {
            state: CircuitState::Closed,
            last_state_change: Instant::now(),
            cached_at: Instant::now(),
        }
    }
}
pub struct DistributedCircuitBreaker {
    name: String,
    config: CircuitBreakerConfig,
    db: Arc<PgPool>,
    local_cache: Arc<RwLock<Option<LocalCacheEntry>>>,
    cache_ttl: Duration,
    node_id: String,
    total_requests: AtomicU64,
    rejected_requests: AtomicU64,
}
impl DistributedCircuitBreaker {
    pub fn new(
        name: String,
        config: CircuitBreakerConfig,
        db: Arc<PgPool>,
        cache_ttl: Duration,
    ) -> Self {
        let node_id = hostname::get().map_or_else(
            |_| "unknown".to_string(),
            |h| h.to_string_lossy().to_string(),
        );
        debug!(
            circuit_breaker = % name, node_id = % node_id, cache_ttl_ms = cache_ttl
            .as_millis(), "Distributed circuit breaker created"
        );
        Self {
            name,
            config,
            db,
            local_cache: Arc::new(RwLock::new(None)),
            cache_ttl,
            node_id,
            total_requests: AtomicU64::new(0),
            rejected_requests: AtomicU64::new(0),
        }
    }
    #[must_use]
    pub fn with_defaults(name: String, config: CircuitBreakerConfig, db: Arc<PgPool>) -> Self {
        Self::new(name, config, db, Duration::from_millis(100))
    }
    async fn invalidate_cache(&self) {
        let mut cache = self.local_cache.write().await;
        *cache = None;
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn state(&self) -> Result<CircuitState, CoreError> {
        let entry = self.get_or_load_state().await?;
        Ok(entry.state)
    }
    async fn get_or_load_state(&self) -> Result<LocalCacheEntry, CoreError> {
        {
            let cache = self.local_cache.read().await;
            if let Some(ref entry) = *cache
                && entry.cached_at.elapsed() < self.cache_ttl
            {
                return Ok(entry.clone());
            }
        }
        self.load_from_db().await
    }
    async fn load_from_db(&self) -> Result<LocalCacheEntry, CoreError> {
        let row = sqlx::query(
            r"
            SELECT state, failure_count, success_count, last_state_change,
                   failure_threshold, success_threshold, cooldown_duration_secs
            FROM mazbri_platform.circuit_breakers
            WHERE name = $1
            ",
        )
        .bind(&self.name)
        .fetch_optional(&*self.db)
        .await
        .map_err(CoreError::Database)?;
        let entry = if let Some(r) = row {
            let state_value: i16 = storage_sync::get_row_field_i16(&r, "state", "load_from_db");
            let state = state_machine::state_from_db_value(state_value);
            let db_last_change: Option<DateTime<Utc>> = r.try_get("last_state_change").ok();
            let last_state_change =
                storage_sync::instant_from_db_timestamp(&self.name, db_last_change);
            storage_sync::build_cache_entry(state, last_state_change)
        } else {
            let entry = LocalCacheEntry::default();
            self.insert_default_record().await?;
            entry
        };
        {
            let mut cache = self.local_cache.write().await;
            *cache = Some(entry.clone());
        }
        Ok(entry)
    }
    async fn insert_default_record(&self) -> Result<(), CoreError> {
        sqlx::query(
            r"
            INSERT INTO mazbri_platform.circuit_breakers
                (name, state, failure_count, success_count, failure_threshold,
                 success_threshold, cooldown_duration_secs, updated_by)
            VALUES ($1, 0, 0, 0, $2, $3, $4, $5)
            ON CONFLICT (name) DO NOTHING
            ",
        )
        .bind(&self.name)
        .bind(i64::saturating_from(self.config.failure_threshold))
        .bind(i64::saturating_from(self.config.success_threshold))
        .bind(i64::saturating_from(
            self.config.cooldown_duration.as_secs(),
        ))
        .bind(&self.node_id)
        .execute(&*self.db)
        .await
        .map_err(CoreError::Database)?;
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn is_request_allowed(&self) -> Result<bool, CoreError> {
        let entry = self.get_or_load_state().await?;
        match entry.state {
            CircuitState::Open => Ok(state_machine::open_state_cooldown_passed(
                entry.last_state_change,
                self.config.cooldown_duration,
            )),
            CircuitState::Closed | CircuitState::HalfOpen => Ok(true),
        }
    }
    pub async fn execute<F, T, Fut>(&self, request_fn: F) -> CircuitResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        let entry = match self.get_or_load_state().await {
            Ok(e) => e,
            Err(e) => {
                warn!(
                    circuit_breaker = % self.name, error = % e,
                    "Failed to load circuit breaker state, allowing request (fail-open)"
                );
                let result = request_fn().await;
                return result.map_or_else(
                    |err| CircuitResult::Failure { error: err },
                    CircuitResult::Success,
                );
            }
        };
        match entry.state {
            CircuitState::Open => self.handle_open_state(&entry).await,
            CircuitState::HalfOpen => {
                let result = request_fn().await;
                self.handle_half_open(result).await
            }
            CircuitState::Closed => {
                let result = request_fn().await;
                self.handle_closed(result).await
            }
        }
    }
    async fn handle_open_state<T>(&self, entry: &LocalCacheEntry) -> CircuitResult<T> {
        let cooldown_passed = state_machine::open_state_cooldown_passed(
            entry.last_state_change,
            self.config.cooldown_duration,
        );
        if cooldown_passed {
            info!(
                circuit_breaker = % self.name,
                "Cooldown period elapsed, transitioning to half-open"
            );
            self.transition_to(CircuitState::HalfOpen).await;
            return CircuitResult::Rejected {
                reason: "Circuit transitioning to half-open".to_string(),
                retry_after: Duration::from_secs(0),
            };
        }
        let retry_after = self
            .config
            .cooldown_duration
            .saturating_sub(entry.last_state_change.elapsed());
        self.rejected_requests.fetch_add(1, Ordering::Relaxed);
        warn!(
            circuit_breaker = % self.name, retry_after_secs = retry_after.as_secs(),
            "Circuit is open, request rejected"
        );
        CircuitResult::Rejected {
            reason: "Circuit is open".to_string(),
            retry_after,
        }
    }
    async fn handle_half_open<T>(&self, result: Result<T, String>) -> CircuitResult<T> {
        match result {
            Ok(value) => {
                let successes = match self.increment_success_count().await {
                    Ok(count) => count,
                    Err(e) => {
                        warn!(
                            circuit_breaker = % self.name, error = % e,
                            "Failed to increment success count, proceeding anyway"
                        );
                        self.config.success_threshold
                    }
                };
                info!(
                    circuit_breaker = % self.name, successes = successes, threshold =
                    self.config.success_threshold, "Half-open request succeeded"
                );
                if successes >= self.config.success_threshold {
                    info!(
                        circuit_breaker = % self.name,
                        "Success threshold reached, closing circuit"
                    );
                    self.transition_to(CircuitState::Closed).await;
                    if let Err(e) = self.reset_counters().await {
                        warn!(
                            circuit_breaker = % self.name, error = % e,
                            "Failed to reset counters after closing circuit"
                        );
                    }
                }
                CircuitResult::Success(value)
            }
            Err(error) => {
                warn!(
                    circuit_breaker = % self.name, error = % error,
                    "Half-open request failed, reopening circuit"
                );
                self.transition_to(CircuitState::Open).await;
                if let Err(e) = self.reset_counters().await {
                    warn!(
                        circuit_breaker = % self.name, error = % e,
                        "Failed to reset counters after reopening circuit"
                    );
                }
                CircuitResult::Failure { error }
            }
        }
    }
    async fn handle_closed<T>(&self, result: Result<T, String>) -> CircuitResult<T> {
        match result {
            Ok(value) => {
                if let Err(e) = self.reset_failure_count().await {
                    warn!(
                        circuit_breaker = % self.name, error = % e,
                        "Failed to reset failure count after success"
                    );
                }
                debug!(circuit_breaker = % self.name, "Closed state request succeeded");
                CircuitResult::Success(value)
            }
            Err(error) => {
                let failures = match self.increment_failure_count().await {
                    Ok(count) => count,
                    Err(e) => {
                        warn!(
                            circuit_breaker = % self.name, error = % e,
                            "Failed to increment failure count, proceeding anyway"
                        );
                        self.config.failure_threshold
                    }
                };
                warn!(
                    circuit_breaker = % self.name, failures = failures, threshold = self
                    .config.failure_threshold, "Closed state request failed"
                );
                if failures >= self.config.failure_threshold {
                    info!(
                        circuit_breaker = % self.name,
                        "Failure threshold reached, opening circuit"
                    );
                    self.transition_to(CircuitState::Open).await;
                    if let Err(e) = self.reset_counters().await {
                        warn!(
                            circuit_breaker = % self.name, error = % e,
                            "Failed to reset counters after opening circuit"
                        );
                    }
                }
                CircuitResult::Failure { error }
            }
        }
    }
    async fn transition_to(&self, new_state: CircuitState) {
        let state_value = new_state.as_metric_value();
        let result = sqlx::query(
            r"
            INSERT INTO mazbri_platform.circuit_breakers
                (name, state, failure_count, success_count, last_state_change, updated_by)
            VALUES ($1, $2, 0, 0, NOW(), $3)
            ON CONFLICT (name) DO UPDATE SET
                state = $2,
                failure_count = 0,
                success_count = 0,
                last_state_change = NOW(),
                updated_by = $3,
                updated_at = NOW()
            ",
        )
        .bind(&self.name)
        .bind(state_value)
        .bind(&self.node_id)
        .execute(&*self.db)
        .await;
        if let Err(e) = result {
            warn!(
                circuit_breaker = % self.name, error = % e,
                "Failed to update circuit breaker state in database"
            );
        }
        self.invalidate_cache().await;
        info!(
            circuit_breaker = % self.name, state = % new_state, node_id = % self.node_id,
            "Circuit breaker state transition"
        );
    }
    async fn increment_failure_count(&self) -> Result<u64, CoreError> {
        let row = sqlx::query(
            r"
            INSERT INTO mazbri_platform.circuit_breakers
                (name, state, failure_count, success_count, updated_by)
            VALUES ($1, 0, 1, 0, $2)
            ON CONFLICT (name) DO UPDATE SET
                failure_count = circuit_breakers.failure_count + 1,
                success_count = 0,
                updated_by = $2,
                updated_at = NOW()
            RETURNING failure_count
            ",
        )
        .bind(&self.name)
        .bind(&self.node_id)
        .fetch_one(&*self.db)
        .await
        .map_err(CoreError::Database)?;
        let count: i64 =
            storage_sync::get_row_field_i64(&row, "failure_count", "increment_failure_count");
        self.invalidate_cache().await;
        Ok(u64::saturating_from(count))
    }
    async fn increment_success_count(&self) -> Result<u64, CoreError> {
        let row = sqlx::query(
            r"
            INSERT INTO mazbri_platform.circuit_breakers
                (name, state, failure_count, success_count, updated_by)
            VALUES ($1, 1, 0, 1, $2)
            ON CONFLICT (name) DO UPDATE SET
                success_count = circuit_breakers.success_count + 1,
                updated_by = $2,
                updated_at = NOW()
            RETURNING success_count
            ",
        )
        .bind(&self.name)
        .bind(&self.node_id)
        .fetch_one(&*self.db)
        .await
        .map_err(CoreError::Database)?;
        let count: i64 =
            storage_sync::get_row_field_i64(&row, "success_count", "increment_success_count");
        self.invalidate_cache().await;
        Ok(u64::saturating_from(count))
    }
    async fn reset_failure_count(&self) -> Result<(), CoreError> {
        sqlx::query(
            r"
            INSERT INTO mazbri_platform.circuit_breakers
                (name, state, failure_count, success_count, updated_by)
            VALUES ($1, 0, 0, 0, $2)
            ON CONFLICT (name) DO UPDATE SET
                failure_count = 0,
                updated_by = $2,
                updated_at = NOW()
            ",
        )
        .bind(&self.name)
        .bind(&self.node_id)
        .execute(&*self.db)
        .await
        .map_err(CoreError::Database)?;
        self.invalidate_cache().await;
        Ok(())
    }
    async fn reset_counters(&self) -> Result<(), CoreError> {
        sqlx::query(
            r"
            UPDATE mazbri_platform.circuit_breakers
            SET failure_count = 0, success_count = 0, updated_by = $2, updated_at = NOW()
            WHERE name = $1
            ",
        )
        .bind(&self.name)
        .bind(&self.node_id)
        .execute(&*self.db)
        .await
        .map_err(CoreError::Database)?;
        self.invalidate_cache().await;
        Ok(())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn reset(&self) -> Result<(), CoreError> {
        self.transition_to(CircuitState::Closed).await;
        self.reset_counters().await?;
        Ok(())
    }
    pub fn stats(&self) -> (u64, u64) {
        (
            self.total_requests.load(Ordering::Relaxed),
            self.rejected_requests.load(Ordering::Relaxed),
        )
    }
}
