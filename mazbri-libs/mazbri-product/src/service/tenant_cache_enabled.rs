//! Resolve `cache_configs.enabled` from the cache DB for product snapshots.
//!
//! `cache_configs` lives in `mazbri_cache` (not the platform DB), so product SQL
//! cannot JOIN it. Gateway/mgmt pass the cache pool; missing pool or missing row
//! means disabled. Results (including negatives) are remembered for 30s, matching
//! `mazbri-cache` `store.rs` `CONFIG_RESOLVE_CACHE_TTL`.

use sqlx::PgPool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::warn;
use uuid::Uuid;

/// Align with `mazbri_cache::store::CONFIG_RESOLVE_CACHE_TTL`.
const CACHE_ENABLED_RESOLVE_TTL: Duration = Duration::from_secs(30);

/// Per-tenant `cache_configs.enabled` lookup with short TTL (incl. negative).
pub(crate) struct TenantCacheEnabledLookup {
    cache_db: Option<PgPool>,
    cache: moka::sync::Cache<Uuid, bool>,
    db_fetches: AtomicU64,
}

impl TenantCacheEnabledLookup {
    pub(crate) fn new(cache_db: Option<PgPool>) -> Self {
        Self {
            cache_db,
            cache: moka::sync::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(CACHE_ENABLED_RESOLVE_TTL)
                .build(),
            db_fetches: AtomicU64::new(0),
        }
    }

    /// Drop one tenant so the next call reloads from Postgres.
    pub(crate) fn invalidate(&self, tenant_id: Uuid) {
        self.cache.invalidate(&tenant_id);
    }

    pub(crate) fn invalidate_all(&self) {
        self.cache.invalidate_all();
    }

    /// How many times Postgres was queried (includes negative-cache fills).
    #[cfg(test)]
    #[must_use]
    pub(crate) fn db_fetch_count(&self) -> u64 {
        self.db_fetches.load(Ordering::Relaxed)
    }

    /// Seed memory cache without hitting Postgres (unit tests).
    #[cfg(test)]
    pub(crate) fn seed_for_test(&self, tenant_id: Uuid, enabled: bool) {
        self.cache.insert(tenant_id, enabled);
    }

    /// Resolve whether response caching is enabled for `tenant_id`.
    ///
    /// No cache DB / no row / `enabled=false` / lookup error → `false` (fail-open
    /// to the skip-RPC path). Successful results are cached for
    /// [`CACHE_ENABLED_RESOLVE_TTL`].
    pub(crate) async fn is_enabled(&self, tenant_id: Uuid) -> bool {
        if let Some(enabled) = self.cache.get(&tenant_id) {
            return enabled;
        }

        let Some(ref pool) = self.cache_db else {
            // No cache DB wired → treat as disabled; remember briefly.
            self.cache.insert(tenant_id, false);
            return false;
        };

        self.db_fetches.fetch_add(1, Ordering::Relaxed);
        match fetch_enabled_from_db(pool, tenant_id).await {
            Ok(enabled) => {
                self.cache.insert(tenant_id, enabled);
                enabled
            }
            Err(error) => {
                warn!(
                    error = %error,
                    tenant_id = %tenant_id,
                    "cache_configs.enabled lookup failed; treating as disabled"
                );
                self.cache.insert(tenant_id, false);
                false
            }
        }
    }
}

async fn fetch_enabled_from_db(pool: &PgPool, tenant_id: Uuid) -> Result<bool, sqlx::Error> {
    let enabled: Option<bool> =
        sqlx::query_scalar("SELECT enabled FROM cache_configs WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_optional(pool)
            .await?;
    // No row → disabled (control-plane switch absent).
    Ok(enabled.unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn no_cache_db_reports_disabled() {
        let lookup = TenantCacheEnabledLookup::new(None);
        let tenant = Uuid::nil();
        assert!(!lookup.is_enabled(tenant).await);
        assert_eq!(lookup.db_fetch_count(), 0, "no pool must not query Postgres");
    }

    #[tokio::test]
    async fn seeded_true_reports_enabled() {
        let lookup = TenantCacheEnabledLookup::new(None);
        let tenant = Uuid::from_u128(1);
        lookup.seed_for_test(tenant, true);
        assert!(lookup.is_enabled(tenant).await);
        assert_eq!(lookup.db_fetch_count(), 0);
    }

    #[tokio::test]
    async fn negative_cache_avoids_second_db_fetch() {
        // Lazy pool: cache-hit path never opens a connection.
        let db = PgPool::connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .expect("lazy pool");
        let lookup = TenantCacheEnabledLookup::new(Some(db));
        let tenant = Uuid::from_u128(2);

        // Seed negative as if DB said "no row / disabled".
        lookup.seed_for_test(tenant, false);
        let before = lookup.db_fetch_count();
        assert!(!lookup.is_enabled(tenant).await);
        assert_eq!(lookup.db_fetch_count(), before);
        assert!(!lookup.is_enabled(tenant).await);
        assert_eq!(lookup.db_fetch_count(), before);
    }

    #[tokio::test]
    async fn invalidate_clears_memory_entry() {
        let lookup = TenantCacheEnabledLookup::new(None);
        let tenant = Uuid::from_u128(3);
        lookup.seed_for_test(tenant, true);
        assert!(lookup.is_enabled(tenant).await);
        lookup.invalidate(tenant);
        // No cache DB → after invalidate, resolve again as disabled without Postgres.
        assert!(!lookup.is_enabled(tenant).await);
        assert_eq!(lookup.db_fetch_count(), 0);
    }
    #[tokio::test]
    async fn enabled_true_fills_snapshot_gate() {
        let lookup = TenantCacheEnabledLookup::new(None);
        let tenant = Uuid::from_u128(42);
        lookup.seed_for_test(tenant, true);
        let enabled = lookup.is_enabled(tenant).await;
        let snapshot = crate::model::TenantResponseCache { enabled };
        // Gateway `should_attempt_cache_rpc` is identity on this flag.
        assert!(
            snapshot.enabled,
            "enabled cache_configs row must open the gateway snapshot gate"
        );
    }

    #[tokio::test]
    async fn no_row_keeps_snapshot_gate_closed() {
        let lookup = TenantCacheEnabledLookup::new(None);
        let tenant = Uuid::from_u128(43);
        let enabled = lookup.is_enabled(tenant).await;
        let snapshot = crate::model::TenantResponseCache { enabled };
        assert!(!snapshot.enabled, "missing cache_configs must keep gate closed");
    }
}
