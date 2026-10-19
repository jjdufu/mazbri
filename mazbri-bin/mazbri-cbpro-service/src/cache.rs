use crate::config::CacheConfig;
use crate::db::{LocalCacheEntry, RedisFallbackRepository, SetCacheParams};
use lru::LruCache;
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
pub struct HybridFallbackCache {
    memory: Mutex<LruCache<String, LocalCacheEntry>>,
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
            if let Some(local_entry) = cache.get(key) {
                if !local_entry.is_local_expired() {
                    debug!(key = % key, source = "L1_memory", "Cache hit");
                    return Some(CacheEntry {
                        data: local_entry.data.clone(),
                        created_at: local_entry.created_at,
                    });
                }
            }
        }
        if let Some(ref redis) = self.redis {
            if let Ok(Some(redis_entry)) = redis.get(key).await {
                if !redis_entry.is_expired() {
                    let local_entry = redis_entry.to_local_entry(self.local_cache_ttl);
                    let mut cache = self.memory.lock();
                    let len_before = cache.len();
                    cache.put(key.to_string(), local_entry);
                    if cache.len() > len_before {
                        self.active_count.fetch_add(1, Ordering::Relaxed);
                    }
                    debug!(key = % key, source = "L2_redis", "Cache hit");
                    let age_secs = redis_entry.age_secs();
                    return Some(CacheEntry {
                        data: redis_entry.data,
                        created_at: Instant::now() - Duration::from_secs(age_secs as u64),
                    });
                }
            }
        }
        debug!(key = % key, "Cache miss");
        None
    }
    pub async fn set(&self, params: SetCacheEntryParams) {
        let ttl = params.ttl.unwrap_or(self.default_ttl);
        let ttl_secs = ttl.as_secs() as i64;
        let local_entry = LocalCacheEntry {
            data: params.data.clone(),
            created_at: Instant::now(),
            local_cached_at: Instant::now(),
            local_ttl: self.local_cache_ttl,
        };
        {
            let mut cache = self.memory.lock();
            if cache.len() >= self.max_entries {
                Self::evict_expired_locked(&mut cache, &self.active_count);
            }
            let len_before = cache.len();
            let _old = cache.put(params.key.clone(), local_entry);
            if _old.is_none() && cache.len() > len_before {
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
                    key = % params.key, error = % e,
                    "Failed to store cache entry in Redis"
                );
            }
        }
        debug!(key = % params.key, ttl_secs = ttl_secs, "Cache entry set");
    }
    fn evict_expired_locked(
        cache: &mut LruCache<String, LocalCacheEntry>,
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
    pub created_at: std::time::Instant,
}
impl CacheEntry {
    pub fn age_secs(&self) -> i64 {
        self.created_at.elapsed().as_secs() as i64
    }
}
