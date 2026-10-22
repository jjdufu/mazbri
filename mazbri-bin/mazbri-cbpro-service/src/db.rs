use anyhow::Result;
use chrono::{DateTime, Utc};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::redis::RedisClient;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::time::{Duration, Instant};
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RedisFallbackEntry {
    data: JsonValue,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct DbCacheEntry {
    pub data: JsonValue,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl DbCacheEntry {
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.expires_at
    }

    pub fn age_secs(&self) -> i64 {
        (chrono::Utc::now() - self.created_at).num_seconds().max(0)
    }

    pub fn to_local_entry(&self, local_ttl: Duration) -> LocalCacheEntry {
        LocalCacheEntry {
            data: self.data.clone(),
            created_at: Instant::now(),
            local_cached_at: Instant::now(),
            local_ttl,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LocalCacheEntry {
    pub data: JsonValue,
    pub created_at: Instant,
    pub local_cached_at: Instant,
    pub local_ttl: Duration,
}

impl LocalCacheEntry {
    pub fn is_local_expired(&self) -> bool {
        self.local_cached_at.elapsed() > self.local_ttl
    }
}

#[derive(Debug, Clone)]
pub struct SetCacheParams<'a> {
    pub cache_key: &'a str,
    pub data: &'a JsonValue,
    pub ttl_secs: i64,
    pub last_refreshed_at: Option<DateTime<Utc>>,
}

pub struct RedisFallbackRepository {
    redis: RedisClient,
    node_id: String,
}

impl RedisFallbackRepository {
    pub fn new(redis: RedisClient) -> Self {
        let node_id = hostname::get().map_or_else(
            |_| "unknown".to_string(),
            |h| h.to_string_lossy().to_string(),
        );
        Self { redis, node_id }
    }

    pub async fn get(&self, cache_key: &str) -> Result<Option<DbCacheEntry>> {
        let redis_key = self.redis.fallback_key(cache_key);
        let row = self
            .redis
            .get_bytes(&redis_key)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        Ok(match row {
            Some(bytes) => {
                let entry: RedisFallbackEntry = serde_json::from_slice(&bytes)?;
                if chrono::Utc::now() > entry.expires_at {
                    None
                } else {
                    Some(DbCacheEntry {
                        data: entry.data,
                        created_at: entry.created_at,
                        expires_at: entry.expires_at,
                    })
                }
            }
            None => None,
        })
    }

    pub async fn set(&self, params: SetCacheParams<'_>) -> Result<()> {
        let ttl_secs = usize::saturating_from(params.ttl_secs.max(1));
        let expires_at = Utc::now() + chrono::Duration::seconds(params.ttl_secs.max(1));
        let entry = RedisFallbackEntry {
            data: params.data.clone(),
            created_at: params.last_refreshed_at.unwrap_or_else(Utc::now),
            expires_at,
        };
        let payload = serde_json::to_vec(&entry)?;
        let redis_key = self.redis.fallback_key(params.cache_key);
        self.redis
            .set_ex_bytes(&redis_key, &payload, ttl_secs)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        debug!(
            cache_key = % params.cache_key, ttl_secs = params.ttl_secs, node_id = % self
            .node_id, "Cache entry stored in Redis"
        );
        Ok(())
    }
}
