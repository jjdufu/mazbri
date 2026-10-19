//! 余额预检缓存：L1 本 Pod 内存 + L2 Redis（多副本共享）+ L3 billing 权威读。
//! 预检允许时做本地预留扣减，避免热路径反复打 billing DB；账本仍以 billing/PG 为准。

use mazbri_core::redis::RedisClient;
use moka::sync::Cache;
use rust_decimal::Decimal;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};
use uuid::Uuid;

const REDIS_KEY_PREFIX: &str = "billing:bal:";

#[derive(Debug, Clone)]
pub struct BalanceCacheConfig {
    pub enabled: bool,
    /// L1 内存 TTL
    pub l1_ttl: Duration,
    /// L2 Redis TTL（秒，Redis SET EX 最小 1）
    pub l2_ttl_secs: u64,
    pub l1_max_capacity: u64,
}

impl Default for BalanceCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            l1_ttl: Duration::from_millis(2000),
            l2_ttl_secs: 5,
            l1_max_capacity: 50_000,
        }
    }
}

#[derive(Clone)]
pub struct BalancePrecheckCache {
    l1: Cache<Uuid, Decimal>,
    redis: RedisClient,
    l2_ttl_secs: usize,
    enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BalanceCacheSource {
    L1,
    L2,
}

impl BalancePrecheckCache {
    pub fn new(redis: RedisClient, config: BalanceCacheConfig) -> Self {
        let l1 = Cache::builder()
            .max_capacity(config.l1_max_capacity)
            .time_to_live(config.l1_ttl)
            .build();
        Self {
            l1,
            redis,
            l2_ttl_secs: config.l2_ttl_secs.max(1) as usize,
            enabled: config.enabled,
        }
    }

    fn redis_key(tenant_id: Uuid) -> String {
        format!("{REDIS_KEY_PREFIX}{tenant_id}")
    }

    /// 返回缓存中的 available_balance（若有）
    pub async fn get_available(&self, tenant_id: Uuid) -> Option<(Decimal, BalanceCacheSource)> {
        if !self.enabled {
            return None;
        }
        if let Some(available) = self.l1.get(&tenant_id) {
            debug!(%tenant_id, %available, source = "l1", "balance precheck cache hit");
            return Some((available, BalanceCacheSource::L1));
        }
        let key = Self::redis_key(tenant_id);
        match self.redis.get_bytes(&key).await {
            Ok(Some(bytes)) => {
                let text = String::from_utf8_lossy(&bytes);
                match Decimal::from_str(text.trim()) {
                    Ok(available) => {
                        self.l1.insert(tenant_id, available);
                        debug!(%tenant_id, %available, source = "l2", "balance precheck redis hit");
                        Some((available, BalanceCacheSource::L2))
                    }
                    Err(error) => {
                        warn!(%tenant_id, error = %error, "invalid balance cache value in redis");
                        let _ = self.redis.del(&key).await;
                        None
                    }
                }
            }
            Ok(None) => None,
            Err(error) => {
                warn!(%tenant_id, error = %error, "balance precheck redis get failed");
                None
            }
        }
    }

    pub async fn put_available(&self, tenant_id: Uuid, available: Decimal) {
        if !self.enabled {
            return;
        }
        self.l1.insert(tenant_id, available);
        let key = Self::redis_key(tenant_id);
        let value = available.normalize().to_string();
        if let Err(error) = self
            .redis
            .set_ex_bytes(&key, value.as_bytes(), self.l2_ttl_secs)
            .await
        {
            warn!(%tenant_id, error = %error, "balance precheck redis set failed");
        }
    }

    pub async fn invalidate(&self, tenant_id: Uuid) {
        if !self.enabled {
            return;
        }
        self.l1.invalidate(&tenant_id);
        let key = Self::redis_key(tenant_id);
        let _ = self.redis.del(&key).await;
    }

    /// 预检通过后预留：从 L1/L2 乐观扣减，降低后续请求打 L3 的概率。
    pub async fn reserve(&self, tenant_id: Uuid, amount: Decimal) {
        self.optimistic_subtract(tenant_id, amount).await;
    }

    /// 入账后的乐观扣减：更新 L1，并写回 L2（多副本共享预留后的值）。
    pub async fn optimistic_subtract(&self, tenant_id: Uuid, amount: Decimal) {
        if !self.enabled || amount <= Decimal::ZERO {
            return;
        }
        let next = if let Some(available) = self.l1.get(&tenant_id) {
            (available - amount).max(Decimal::ZERO)
        } else {
            // No L1: drop L2 so next miss reloads from billing instead of serving a stale high value.
            let key = Self::redis_key(tenant_id);
            let _ = self.redis.del(&key).await;
            return;
        };
        self.l1.insert(tenant_id, next);
        let key = Self::redis_key(tenant_id);
        let value = next.normalize().to_string();
        if let Err(error) = self
            .redis
            .set_ex_bytes(&key, value.as_bytes(), self.l2_ttl_secs)
            .await
        {
            // L1 still holds the reservation; L2 miss is acceptable.
            warn!(%tenant_id, error = %error, "balance precheck redis reserve set failed");
        }
    }
}

/// 与 BillingService 默认 min_balance_ratio=0.1 对齐：
/// required = estimated_cost + estimated_cost * 0.1 = estimated_cost * 1.1
pub fn required_available(estimated_cost: Decimal) -> Decimal {
    let ratio = Decimal::from_str_exact("0.1").unwrap_or(Decimal::ZERO);
    estimated_cost + estimated_cost * ratio
}

pub type SharedBalancePrecheckCache = Arc<BalancePrecheckCache>;
