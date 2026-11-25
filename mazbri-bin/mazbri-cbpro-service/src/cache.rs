use crate::config::CacheConfig;
use crate::db::{DbCacheEntry, LocalCacheEntry, RedisFallbackRepository, SetCacheParams};
use lru::LruCache;
use mazbri_core::num::SaturatingFrom;
use parking_lot::Mutex;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct SetCacheEntryParams {
    pub key: String,
    pub data: serde_json::Value,
    pub ttl: Option<Duration>,
}

#[derive(Debug, Clone)]
struct L1CacheEntry {
    inner: LocalCacheEntry,
    stored_at: Instant,
    initial_age: Duration,
}

impl L1CacheEntry {
    fn new_fresh(data: serde_json::Value, local_ttl: Duration) -> Self {
        let now = Instant::now();
        Self {
            inner: LocalCacheEntry {
                data,
                created_at: now,
                local_cached_at: now,
                local_ttl,
            },
            stored_at: now,
            initial_age: Duration::ZERO,
        }
    }

    fn from_db_entry(redis_entry: &DbCacheEntry, local_ttl: Duration) -> Self {
        let inner = redis_entry.to_local_entry(local_ttl);
        let stored_at = inner.created_at;
        let initial_age = Duration::from_secs(u64::saturating_from(redis_entry.age_secs()));
        Self {
            inner,
            stored_at,
            initial_age,
        }
    }

    fn is_local_expired(&self) -> bool {
        self.inner.is_local_expired()
    }

    fn to_cache_entry(&self) -> CacheEntry {
        CacheEntry {
            data: self.inner.data.clone(),
            stored_at: self.stored_at,
            initial_age: self.initial_age,
        }
    }
}

pub struct HybridFallbackCache {
    memory: Mutex<LruCache<String, L1CacheEntry>>,
    redis: Option<Arc<RedisFallbackRepository>>,
    max_entries: usize,
    default_ttl: Duration,
    local_cache_ttl: Duration,
    active_count: AtomicUsize,
}

impl HybridFallbackCache {
    pub fn new(config: &CacheConfig, redis: Option<Arc<RedisFallbackRepository>>) -> Self {
        let cap = NonZeroUsize::new(config.max_entries)
            .unwrap_or_else(|| NonZeroUsize::new(100).expect("100 is non-zero"));
        if redis.is_some() {
            info!(
                max_entries = config.max_entries,
                default_ttl_secs = config.default_ttl_secs,
                local_cache_ttl_ms = config.local_cache_ttl_ms,
                "Hybrid fallback cache initialized with Redis persistence"
            );
        } else {
            info!(
                max_entries = config.max_entries,
                default_ttl_secs = config.default_ttl_secs,
                "Fallback cache initialized in memory-only mode"
            );
        }
        Self {
            memory: Mutex::new(LruCache::new(cap)),
            redis,
            max_entries: config.max_entries,
            default_ttl: config.default_ttl(),
            local_cache_ttl: config.local_cache_ttl(),
            active_count: AtomicUsize::new(0),
        }
    }

    pub async fn get(&self, key: &str) -> Option<CacheEntry> {
        {
            let mut cache = self.memory.lock();
            if let Some(local_entry) = cache.get(key)
                && !local_entry.is_local_expired()
            {
                debug!(key = %key, source = "L1_memory", "Cache hit");
                return Some(local_entry.to_cache_entry());
            }
        }
        if let Some(ref redis) = self.redis
            && let Ok(Some(redis_entry)) = redis.get(key).await
            && !redis_entry.is_expired()
        {
            return Some(self.backfill_l1_from_db_entry(key, &redis_entry));
        }
        debug!(key = %key, "Cache miss");
        None
    }

    fn backfill_l1_from_db_entry(&self, key: &str, redis_entry: &DbCacheEntry) -> CacheEntry {
        let l1_entry = L1CacheEntry::from_db_entry(redis_entry, self.local_cache_ttl);
        let out = l1_entry.to_cache_entry();
        let mut cache = self.memory.lock();
        let len_before = cache.len();
        cache.put(key.to_string(), l1_entry);
        if cache.len() > len_before {
            self.active_count.fetch_add(1, Ordering::Relaxed);
        }
        debug!(key = %key, source = "L2_redis", "Cache hit");
        out
    }

    pub async fn set(&self, params: SetCacheEntryParams) {
        let ttl = params.ttl.unwrap_or(self.default_ttl);
        let ttl_secs = i64::saturating_from(ttl.as_secs());
        let l1_entry = L1CacheEntry::new_fresh(params.data.clone(), self.local_cache_ttl);
        {
            let mut cache = self.memory.lock();
            if cache.len() >= self.max_entries {
                Self::evict_expired_locked(&mut cache, &self.active_count);
            }
            let len_before = cache.len();
            let old = cache.put(params.key.clone(), l1_entry);
            if old.is_none() && cache.len() > len_before {
                self.active_count.fetch_add(1, Ordering::Relaxed);
            }
        }
        if let Some(ref redis) = self.redis {
            let db_params = SetCacheParams {
                cache_key: &params.key,
                data: &params.data,
                ttl_secs,
                last_refreshed_at: Some(chrono::Utc::now()),
            };
            if let Err(e) = redis.set(db_params).await {
                warn!(
                    key = %params.key,
                    error = %e,
                    "Failed to store cache entry in Redis"
                );
            }
        }
        debug!(key = %params.key, ttl_secs = ttl_secs, "Cache entry set");
    }

    fn evict_expired_locked(
        cache: &mut LruCache<String, L1CacheEntry>,
        active_count: &AtomicUsize,
    ) {
        let expired_keys: Vec<String> = cache
            .iter()
            .filter(|(_, entry)| entry.is_local_expired())
            .map(|(k, _)| k.clone())
            .collect();
        let evicted = expired_keys.len();
        for key in &expired_keys {
            cache.pop(key);
        }
        if evicted > 0 {
            debug!(
                evicted_count = evicted,
                "Evicted expired entries from memory"
            );
            let new_count = cache.iter().filter(|(_, e)| !e.is_local_expired()).count();
            active_count.store(new_count, Ordering::Relaxed);
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub data: serde_json::Value,
    pub stored_at: Instant,
    pub initial_age: Duration,
}

impl CacheEntry {
    #[must_use]
    pub fn age(&self) -> Duration {
        self.initial_age.saturating_add(self.stored_at.elapsed())
    }

    #[must_use]
    pub fn age_secs(&self) -> i64 {
        i64::saturating_from(self.age().as_secs())
    }
}

#[cfg(test)]
mod tests {
    use super::{CacheEntry, HybridFallbackCache, SetCacheEntryParams};
    use crate::config::CacheConfig;
    use crate::db::DbCacheEntry;
    use chrono::Utc;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn backfill_thirty_day_old_entry_never_underflows_and_preserves_age_in_l1() {
        let config = CacheConfig {
            max_entries: 16,
            default_ttl_secs: 86_400 * 60,
            local_cache_ttl_ms: 5_000,
        };
        let cache = HybridFallbackCache::new(&config, None);
        let now = Utc::now();
        let thirty_days_secs: i64 = 30 * 86_400;
        let db_entry = DbCacheEntry {
            data: serde_json::json!({"ok": true}),
            created_at: now - chrono::TimeDelta::days(30),
            expires_at: now + chrono::TimeDelta::days(30),
        };

        let backfilled = cache.backfill_l1_from_db_entry("tenant:prod:k1", &db_entry);
        assert!(backfilled.age_secs() >= thirty_days_secs);
        assert!(backfilled.age_secs() <= thirty_days_secs + 2);

        let l1_hit = cache
            .get("tenant:prod:k1")
            .await
            .expect("L1 cache should hit within local_cache_ttl");
        assert_eq!(l1_hit.data, serde_json::json!({"ok": true}));
        assert!(l1_hit.age_secs() >= thirty_days_secs);
        assert!(l1_hit.age_secs() <= thirty_days_secs + 2);
    }

    #[tokio::test]
    async fn set_and_get_tracks_monotonic_elapsed_age() {
        let config = CacheConfig {
            max_entries: 8,
            default_ttl_secs: 60,
            local_cache_ttl_ms: 5_000,
        };
        let cache = HybridFallbackCache::new(&config, None);
        cache
            .set(SetCacheEntryParams {
                key: "k-fresh".to_string(),
                data: serde_json::json!({"v": 1}),
                ttl: Some(Duration::from_secs(60)),
            })
            .await;

        let hit = cache.get("k-fresh").await.expect("L1 hit");
        assert_eq!(hit.initial_age, Duration::ZERO);
        assert!(hit.age() < Duration::from_secs(2));

        let synthetic = CacheEntry {
            data: serde_json::json!(null),
            stored_at: Instant::now(),
            initial_age: Duration::MAX,
        };
        assert_eq!(synthetic.age(), Duration::MAX);
    }
}
