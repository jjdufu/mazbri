//! 余额预检缓存：L1 本 Pod 内存 + L2 Redis（多副本共享微元定点数）+ L3 billing 权威读。
//!
//! 严格遵循单次预占 + 响应后差额结算（`reserve(estimated)` -> `settle(estimated, actual)` / `release(estimated)`），
//! Redis L2 通过原子 Lua 脚本执行 `DECRBY` / `INCRBY` **保留既有 TTL 不续期**，确保每 `l2_ttl_secs`
//! 强制回源 PG 权威账本；管理面充值/调账通过 `{prefix}:billing:bal_inv` Pub/Sub 定向剔除单租户 L1 记录。

use dashmap::DashMap;
use mazbri_core::num::SaturatingFrom;
use mazbri_core::redis::RedisClient;
use moka::sync::Cache;
use rust_decimal::Decimal;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, warn};
use uuid::Uuid;

const MICRO_SCALE: u32 = 6;
const MICRO_FACTOR_I128: i128 = 1_000_000;
const L3_RECHECK_MIN_INTERVAL: Duration = Duration::from_secs(1);

/// Atomically decrements an existing L2 micro-yuan key WITHOUT renewing its TTL.
const RESERVE_LUA: &str = r"
if redis.call('EXISTS', KEYS[1]) == 0 then
    return false
end
return redis.call('DECRBY', KEYS[1], tonumber(ARGV[1]))
";

/// Atomically adjusts (`+refund` or `-extra_debit`) an existing L2 micro-yuan key WITHOUT renewing its TTL.
const ADJUST_LUA: &str = r"
if redis.call('EXISTS', KEYS[1]) == 0 then
    return false
end
return redis.call('INCRBY', KEYS[1], tonumber(ARGV[1]))
";

#[derive(Debug, Clone)]
pub struct BalanceCacheConfig {
    pub enabled: bool,
    /// L1 内存 TTL
    pub l1_ttl: Duration,
    /// L2 Redis TTL（秒，Redis SET EX 最小 1，默认 10s）
    pub l2_ttl_secs: u64,
    pub l1_max_capacity: u64,
}

impl Default for BalanceCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            l1_ttl: Duration::from_millis(5000),
            l2_ttl_secs: 10,
            l1_max_capacity: 50_000,
        }
    }
}

#[derive(Clone)]
pub struct BalancePrecheckCache {
    l1: Cache<Uuid, i64>,
    last_l3_fetch: Cache<Uuid, Instant>,
    l3_recheck_locks: Arc<DashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>,
    redis: RedisClient,
    l2_ttl_secs: usize,
    enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BalanceCacheSource {
    L1,
    L2,
}

#[must_use]
pub fn decimal_to_micros(value: Decimal) -> i64 {
    let scaled = (value * Decimal::from(MICRO_FACTOR_I128)).round();
    let mantissa = scaled.trunc().mantissa();
    i64::try_from(mantissa).unwrap_or_else(|_| {
        if mantissa.is_negative() {
            i64::MIN
        } else {
            i64::MAX
        }
    })
}

#[must_use]
pub fn micros_to_decimal(micros: i64) -> Decimal {
    Decimal::new(micros, MICRO_SCALE)
}

impl BalancePrecheckCache {
    #[must_use]
    pub fn new(redis: RedisClient, config: &BalanceCacheConfig) -> Self {
        let l1 = Cache::builder()
            .max_capacity(config.l1_max_capacity)
            .time_to_live(config.l1_ttl)
            .build();
        let last_l3_fetch = Cache::builder()
            .max_capacity(config.l1_max_capacity)
            .time_to_idle(Duration::from_secs(600))
            .build();
        let cache = Self {
            l1,
            last_l3_fetch,
            l3_recheck_locks: Arc::new(DashMap::new()),
            redis,
            l2_ttl_secs: usize::saturating_from(config.l2_ttl_secs.max(1)),
            enabled: config.enabled,
        };
        if cache.enabled && tokio::runtime::Handle::try_current().is_ok() {
            cache.spawn_invalidation_subscriber();
        }
        cache
    }

    fn spawn_invalidation_subscriber(&self) {
        let channel = self.redis.billing_balance_invalidation_channel();
        let this = self.clone();
        drop(self.redis.spawn_pubsub_listener(
            channel,
            "gateway-balance-inv",
            move |payload: String| {
                this.apply_invalidation_payload(&payload);
            },
        ));
    }

    /// Applies a `{prefix}:billing:bal_inv` Pub/Sub payload, evicting ONLY the target tenant's L1 entry.
    pub fn apply_invalidation_payload(&self, payload: &str) {
        let trimmed = payload.trim();
        if trimmed == "*" {
            self.l1.invalidate_all();
            self.last_l3_fetch.invalidate_all();
            return;
        }
        if let Ok(tenant_id) = Uuid::parse_str(trimmed) {
            self.l1.invalidate(&tenant_id);
            self.last_l3_fetch.invalidate(&tenant_id);
            debug!(%tenant_id, "Evicted tenant L1 balance cache via Pub/Sub");
        }
    }

    fn redis_key(&self, tenant_id: Uuid) -> String {
        self.redis.billing_balance_key(tenant_id)
    }

    /// Returns `true` when the last authoritative L3 fetch for `tenant_id` was more than 1s ago
    /// (or has never occurred), allowing a singleflight L3 recheck before rejecting with `402`.
    #[must_use]
    pub fn should_recheck_l3(&self, tenant_id: Uuid) -> bool {
        self.last_l3_fetch
            .get(&tenant_id)
            .is_none_or(|fetched_at| fetched_at.elapsed() > L3_RECHECK_MIN_INTERVAL)
    }

    #[must_use]
    pub fn acquire_l3_recheck_gate(&self, tenant_id: Uuid) -> Arc<tokio::sync::Mutex<()>> {
        Arc::clone(
            self.l3_recheck_locks
                .entry(tenant_id)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .value(),
        )
    }

    pub fn release_l3_recheck_gate(&self, tenant_id: Uuid) {
        self.l3_recheck_locks
            .remove_if(&tenant_id, |_, gate| Arc::strong_count(gate) <= 2);
    }

    /// 返回缓存中的 `available_balance`（若有）。
    pub async fn get_available(&self, tenant_id: Uuid) -> Option<(Decimal, BalanceCacheSource)> {
        if !self.enabled {
            return None;
        }
        if let Some(micros) = self.l1.get(&tenant_id) {
            let available = micros_to_decimal(micros);
            debug!(%tenant_id, %available, source = "l1", "balance precheck cache hit");
            return Some((available, BalanceCacheSource::L1));
        }
        let key = self.redis_key(tenant_id);
        match self.redis.get_bytes(&key).await {
            Ok(Some(bytes)) => {
                let text = String::from_utf8_lossy(&bytes);
                let parsed_micros = text
                    .trim()
                    .parse::<i64>()
                    .ok()
                    .or_else(|| Decimal::from_str(text.trim()).ok().map(decimal_to_micros));
                if let Some(micros) = parsed_micros {
                    self.l1.insert(tenant_id, micros);
                    let available = micros_to_decimal(micros);
                    debug!(%tenant_id, %available, source = "l2", "balance precheck redis hit");
                    Some((available, BalanceCacheSource::L2))
                } else {
                    warn!(%tenant_id, "invalid balance cache value in redis");
                    let _ = self.redis.del(&key).await;
                    None
                }
            }
            Ok(None) => None,
            Err(error) => {
                warn!(%tenant_id, error = %error, "balance precheck redis get failed");
                None
            }
        }
    }

    /// Writes an authoritative L3 balance into L1 and L2 (setting `EX l2_ttl_secs` ONLY here).
    pub async fn put_available(&self, tenant_id: Uuid, available: Decimal) {
        if !self.enabled {
            return;
        }
        let micros = decimal_to_micros(available);
        self.l1.insert(tenant_id, micros);
        self.last_l3_fetch.insert(tenant_id, Instant::now());
        let key = self.redis_key(tenant_id);
        let value = micros.to_string();
        if let Err(error) = self
            .redis
            .set_ex_bytes(&key, value.as_bytes(), self.l2_ttl_secs)
            .await
        {
            warn!(%tenant_id, error = %error, "balance precheck redis set failed");
        }
    }

    /// 请求前单次预占：原子扣减 L2（不续期 TTL）并同步更新 L1。
    pub async fn reserve(&self, tenant_id: Uuid, estimated: Decimal) {
        if !self.enabled || estimated <= Decimal::ZERO {
            return;
        }
        let debit_micros = decimal_to_micros(estimated);
        if debit_micros <= 0 {
            return;
        }
        let key = self.redis_key(tenant_id);
        let arg = debit_micros.to_string();
        match self
            .redis
            .eval_opt_i64(
                RESERVE_LUA,
                std::slice::from_ref(&key),
                std::slice::from_ref(&arg),
            )
            .await
        {
            Ok(Some(new_l2_micros)) => {
                self.l1.insert(tenant_id, new_l2_micros);
                return;
            }
            Ok(None) => {}
            Err(error) => {
                warn!(%tenant_id, error = %error, "balance precheck redis reserve lua failed");
            }
        }
        if let Some(current_l1) = self.l1.get(&tenant_id) {
            self.l1
                .insert(tenant_id, current_l1.saturating_sub(debit_micros));
        }
    }

    /// 非计费响应（4xx/5xx/缓存命中/幂等重放/异常提前中止）全额归还请求前预占额度，保留既有 TTL。
    pub async fn release(&self, tenant_id: Uuid, reserved: Decimal) {
        self.settle(tenant_id, reserved, Decimal::ZERO).await;
    }

    /// 响应后差额结算：
    /// - `actual_billable == reserved`：预占已精确扣减，零额外操作（彻底消除双扣！）；
    /// - `actual_billable < reserved`：归还差额 `reserved - actual_billable`；
    /// - `actual_billable > reserved`：补扣差额 `actual_billable - reserved`。
    pub async fn settle(&self, tenant_id: Uuid, reserved: Decimal, actual_billable: Decimal) {
        if !self.enabled {
            return;
        }
        let reserved_micros = decimal_to_micros(reserved.max(Decimal::ZERO));
        let actual_micros = decimal_to_micros(actual_billable.max(Decimal::ZERO));
        let delta_micros = reserved_micros.saturating_sub(actual_micros);
        if delta_micros == 0 {
            return;
        }
        let key = self.redis_key(tenant_id);
        let arg = delta_micros.to_string();
        match self
            .redis
            .eval_opt_i64(
                ADJUST_LUA,
                std::slice::from_ref(&key),
                std::slice::from_ref(&arg),
            )
            .await
        {
            Ok(Some(new_l2_micros)) => {
                self.l1.insert(tenant_id, new_l2_micros);
                return;
            }
            Ok(None) => {}
            Err(error) => {
                warn!(%tenant_id, error = %error, "balance precheck redis settle lua failed");
            }
        }
        if let Some(current_l1) = self.l1.get(&tenant_id) {
            self.l1
                .insert(tenant_id, current_l1.saturating_add(delta_micros));
        }
    }
}

/// 与 `BillingService` 默认 `min_balance_ratio=0.1` 对齐：
/// required = `estimated_cost` + `estimated_cost` * 0.1 = `estimated_cost` * 1.1
#[must_use]
pub fn required_available(estimated_cost: Decimal) -> Decimal {
    let ratio = Decimal::from_str_exact("0.1").unwrap_or(Decimal::ZERO);
    estimated_cost + estimated_cost * ratio
}

pub type SharedBalancePrecheckCache = Arc<BalancePrecheckCache>;

#[cfg(test)]
mod tests {
    use super::*;

    fn disconnected_cache() -> BalancePrecheckCache {
        let redis = RedisClient::unconnected("redis://127.0.0.1:1").expect("parse redis url");
        BalancePrecheckCache::new(
            redis,
            &BalanceCacheConfig {
                enabled: true,
                l1_ttl: Duration::from_secs(5),
                l2_ttl_secs: 10,
                l1_max_capacity: 1000,
            },
        )
    }

    #[test]
    fn micro_yuan_roundtrip_is_lossless_to_six_decimals() {
        for raw in ["0", "0.000001", "0.001", "10.123456", "99999999.999999"] {
            let dec = Decimal::from_str_exact(raw).expect("decimal");
            assert_eq!(micros_to_decimal(decimal_to_micros(dec)).normalize(), dec.normalize());
        }
    }

    #[tokio::test]
    async fn reserve_then_settle_equal_does_not_double_subtract_and_release_restores() {
        let cache = disconnected_cache();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        cache
            .put_available(tenant_a, Decimal::from_str_exact("1.000000").unwrap())
            .await;
        cache
            .put_available(tenant_b, Decimal::from_str_exact("5.000000").unwrap())
            .await;

        let est = Decimal::from_str_exact("0.100000").unwrap();
        // Billable 200 OK: reserve(0.1) + settle(0.1, 0.1) => net deduction is 0.1, NOT 0.2!
        cache.reserve(tenant_a, est).await;
        cache.settle(tenant_a, est, est).await;
        let (after_billable, _) = cache.get_available(tenant_a).await.expect("hit l1");
        assert_eq!(after_billable, Decimal::from_str_exact("0.900000").unwrap());

        // Non-billable 4xx/5xx/replay: reserve(0.1) + release(0.1) => balance fully restored!
        cache.reserve(tenant_a, est).await;
        cache.release(tenant_a, est).await;
        let (after_release, _) = cache.get_available(tenant_a).await.expect("hit l1");
        assert_eq!(after_release, Decimal::from_str_exact("0.900000").unwrap());

        // Targeted Pub/Sub invalidation evicts ONLY tenant_a, keeping tenant_b warm.
        cache.apply_invalidation_payload(&tenant_a.to_string());
        assert!(cache.get_available(tenant_a).await.is_none());
        assert!(cache.get_available(tenant_b).await.is_some());
    }
}
