//! 余额预检缓存：L1 本 Pod 原子槽位 + L2 Redis（后台微批 Pipeline + `EVALSHA` 共享微元定点数）+ L3 billing 权威读。
//!
//! 严格遵循单次预占 + 响应后差额结算（`reserve(estimated)` -> `settle(estimated, actual)` / `release(estimated)`）：
//! - 热路径 `reserve` / `settle` / `release` 纯内存 `AtomicI64` 完成（`0` 网络 IO），净增量累加进 `pending_l2_deltas`；
//! - 后台协程按 `25 ms` 窗口（或满 64 个租户）通过单条 Redis Pipeline + `EVALSHA` 同步 L2，保留既有 TTL 不续期，
//!   并将返回值对齐回对应租户 L1 槽位，确保每 `l2_ttl_secs` 强制回源 PG 权威账本；
//! - 管理面充值/调账通过 `{prefix}:billing:bal_inv` Pub/Sub 原子剔除目标租户 L1 槽位与待刷增量。

use dashmap::DashMap;
use mazbri_core::num::SaturatingFrom;
use mazbri_core::redis::RedisClient;
use moka::sync::Cache;
use parking_lot::Mutex;
use rust_decimal::Decimal;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tracing::{debug, warn};
use uuid::Uuid;

const MICRO_SCALE: u32 = 6;
const MICRO_FACTOR_I128: i128 = 1_000_000;
const L3_RECHECK_MIN_INTERVAL: Duration = Duration::from_secs(1);
const L2_FLUSH_WINDOW: Duration = Duration::from_millis(25);
const L2_FLUSH_BATCH_SIZE: usize = 64;

/// Atomically adjusts (`+refund` or `-debit`) an existing L2 micro-yuan key WITHOUT renewing its TTL.
const ADJUST_LUA: &str = r"
if redis.call('EXISTS', KEYS[1]) == 0 then
    return false
end
return redis.call('INCRBY', KEYS[1], tonumber(ARGV[1]))
";

#[derive(Debug)]
struct L1BalanceSlot {
    micros: AtomicI64,
    last_l2_micros: AtomicI64,
    epoch: u64,
}

#[derive(Debug, Default)]
struct PendingTenantDelta {
    delta: AtomicI64,
    queued: AtomicBool,
}

#[derive(Debug, Clone, Copy)]
struct FlushedDeltaEntry {
    tenant_id: Uuid,
    delta: i64,
    epoch: Option<u64>,
}

struct BalanceCacheInner {
    l1: Cache<Uuid, Arc<L1BalanceSlot>>,
    last_l3_fetch: Cache<Uuid, Instant>,
    l3_recheck_locks: DashMap<Uuid, Arc<tokio::sync::Mutex<()>>>,
    pending_l2_deltas: DashMap<Uuid, PendingTenantDelta>,
    dirty_queue: Mutex<Vec<Uuid>>,
    tenant_epochs: DashMap<Uuid, u64>,
    next_epoch: AtomicU64,
    global_min_epoch: AtomicU64,
    flush_wake_notify: Notify,
    flush_urgent_notify: Notify,
    redis: RedisClient,
    l2_ttl_secs: usize,
    enabled: bool,
}

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
    inner: Arc<BalanceCacheInner>,
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

impl BalanceCacheInner {
    fn allocate_tenant_epoch(&self, tenant_id: Uuid) -> u64 {
        let epoch = self.next_epoch.fetch_add(1, Ordering::AcqRel) + 1;
        self.tenant_epochs.insert(tenant_id, epoch);
        epoch
    }

    fn current_tenant_epoch(&self, tenant_id: Uuid) -> Option<u64> {
        self.l1
            .get(&tenant_id)
            .map(|slot| slot.epoch)
            .or_else(|| self.tenant_epochs.get(&tenant_id).map(|e| *e))
    }

    fn is_epoch_current(&self, tenant_id: Uuid, batch_epoch: Option<u64>) -> bool {
        let min_epoch = self.global_min_epoch.load(Ordering::Acquire);
        match batch_epoch {
            Some(ep) => {
                ep >= min_epoch && self.tenant_epochs.get(&tenant_id).is_none_or(|cur| *cur == ep)
            }
            None => self
                .tenant_epochs
                .get(&tenant_id)
                .is_none_or(|cur| *cur < min_epoch),
        }
    }

    fn redis_key(&self, tenant_id: Uuid) -> String {
        self.redis.billing_balance_key(tenant_id)
    }

    fn pending_delta(&self, tenant_id: Uuid) -> i64 {
        self.pending_l2_deltas
            .get(&tenant_id)
            .map_or(0, |entry| entry.delta.load(Ordering::Acquire))
    }

    fn record_delta(&self, tenant_id: Uuid, delta_micros: i64) {
        if delta_micros == 0 {
            return;
        }
        if let Some(slot) = self.l1.get(&tenant_id) {
            slot.micros.fetch_add(delta_micros, Ordering::AcqRel);
        }
        let became_queued = if let Some(entry) = self.pending_l2_deltas.get(&tenant_id) {
            let prev = entry.delta.fetch_add(delta_micros, Ordering::SeqCst);
            prev.saturating_add(delta_micros) != 0 && !entry.queued.swap(true, Ordering::SeqCst)
        } else {
            let entry = self.pending_l2_deltas.entry(tenant_id).or_default();
            let prev = entry.delta.fetch_add(delta_micros, Ordering::SeqCst);
            prev.saturating_add(delta_micros) != 0 && !entry.queued.swap(true, Ordering::SeqCst)
        };
        if became_queued {
            let queue_len = {
                let mut q = self.dirty_queue.lock();
                q.push(tenant_id);
                q.len()
            };
            if queue_len >= L2_FLUSH_BATCH_SIZE {
                self.flush_urgent_notify.notify_one();
            } else {
                self.flush_wake_notify.notify_one();
            }
        }
    }

    fn drain_dirty_batches(&self) -> Vec<Vec<FlushedDeltaEntry>> {
        let dirty_ids = {
            let mut q = self.dirty_queue.lock();
            if q.is_empty() {
                return Vec::new();
            }
            std::mem::take(&mut *q)
        };
        let mut collected = Vec::with_capacity(dirty_ids.len());
        for tenant_id in dirty_ids {
            let Some(entry) = self.pending_l2_deltas.get(&tenant_id) else {
                continue;
            };
            entry.queued.store(false, Ordering::SeqCst);
            let delta = entry.delta.swap(0, Ordering::SeqCst);
            if delta == 0 {
                continue;
            }
            let epoch = self.current_tenant_epoch(tenant_id);
            collected.push(FlushedDeltaEntry {
                tenant_id,
                delta,
                epoch,
            });
        }
        collected
            .chunks(L2_FLUSH_BATCH_SIZE)
            .map(<[FlushedDeltaEntry]>::to_vec)
            .collect()
    }

    fn apply_l2_batch_results(&self, batch: &[FlushedDeltaEntry], results: Vec<Option<i64>>) {
        for (item, maybe_l2) in batch.iter().zip(results) {
            if !self.is_epoch_current(item.tenant_id, item.epoch) {
                continue;
            }
            match maybe_l2 {
                Some(new_l2_micros) => {
                    if let Some(slot) = self.l1.get(&item.tenant_id)
                        && Some(slot.epoch) == item.epoch
                    {
                        let prev_l2 = slot.last_l2_micros.swap(new_l2_micros, Ordering::AcqRel);
                        let expected_l2 = prev_l2.saturating_add(item.delta);
                        let external_drift = new_l2_micros.saturating_sub(expected_l2);
                        if external_drift != 0 {
                            slot.micros.fetch_add(external_drift, Ordering::AcqRel);
                        }
                    }
                }
                None => {
                    // L2 key expired (`EXISTS == 0`); evict L1 slot at the same epoch so 10s L3 refresh holds.
                    if let Some(slot) = self.l1.get(&item.tenant_id)
                        && Some(slot.epoch) == item.epoch
                    {
                        self.l1.invalidate(&item.tenant_id);
                    }
                }
            }
        }
    }

    async fn flush_all_pending_to_redis(&self) {
        let batches = self.drain_dirty_batches();
        if batches.is_empty() || !self.redis.has_active_connections() {
            return;
        }
        for batch in batches {
            let keys_and_args: Vec<(String, i64)> = batch
                .iter()
                .map(|item| (self.redis_key(item.tenant_id), item.delta))
                .collect();
            match self
                .redis
                .evalsha_batch_opt_i64(ADJUST_LUA, &keys_and_args)
                .await
            {
                Ok(results) => self.apply_l2_batch_results(&batch, results),
                Err(error) => {
                    warn!(
                        batch_len = keys_and_args.len(),
                        error = %error,
                        "balance precheck redis batch evalsha failed"
                    );
                }
            }
        }
    }
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
        let inner = Arc::new(BalanceCacheInner {
            l1,
            last_l3_fetch,
            l3_recheck_locks: DashMap::new(),
            pending_l2_deltas: DashMap::new(),
            dirty_queue: Mutex::new(Vec::new()),
            tenant_epochs: DashMap::new(),
            next_epoch: AtomicU64::new(1),
            global_min_epoch: AtomicU64::new(0),
            flush_wake_notify: Notify::new(),
            flush_urgent_notify: Notify::new(),
            redis,
            l2_ttl_secs: usize::saturating_from(config.l2_ttl_secs.max(1)),
            enabled: config.enabled,
        });
        let cache = Self { inner };
        if cache.inner.enabled && tokio::runtime::Handle::try_current().is_ok() {
            cache.spawn_invalidation_subscriber();
            cache.spawn_l2_flush_worker();
        }
        cache
    }

    fn spawn_invalidation_subscriber(&self) {
        let channel = self.inner.redis.billing_balance_invalidation_channel();
        let this = self.clone();
        drop(self.inner.redis.spawn_pubsub_listener(
            channel,
            "gateway-balance-inv",
            move |payload: String| {
                this.apply_invalidation_payload(&payload);
            },
        ));
    }

    fn spawn_l2_flush_worker(&self) {
        if !self.inner.redis.has_active_connections() {
            return;
        }
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            loop {
                let Some(inner) = weak.upgrade() else {
                    break;
                };
                let has_pending = !inner.dirty_queue.lock().is_empty();
                if !has_pending {
                    let wake = inner.flush_wake_notify.notified();
                    let urgent = inner.flush_urgent_notify.notified();
                    tokio::select! {
                        () = wake => {}
                        () = urgent => {}
                    }
                    let urgent_wait = inner.flush_urgent_notify.notified();
                    let need_window_wait = inner.dirty_queue.lock().len() < L2_FLUSH_BATCH_SIZE;
                    if need_window_wait {
                        tokio::select! {
                            () = tokio::time::sleep(L2_FLUSH_WINDOW) => {}
                            () = urgent_wait => {}
                        }
                    }
                }
                inner.flush_all_pending_to_redis().await;
            }
        });
    }

    /// Applies a `{prefix}:billing:bal_inv` Pub/Sub payload, atomically evicting target L1 slot & pending delta.
    pub fn apply_invalidation_payload(&self, payload: &str) {
        let trimmed = payload.trim();
        if trimmed == "*" {
            let barrier = self.inner.next_epoch.fetch_add(1, Ordering::AcqRel) + 1;
            self.inner.global_min_epoch.store(barrier, Ordering::Release);
            self.inner.tenant_epochs.clear();
            self.inner.dirty_queue.lock().clear();
            self.inner.pending_l2_deltas.clear();
            self.inner.l1.invalidate_all();
            self.inner.last_l3_fetch.invalidate_all();
            return;
        }
        if let Ok(tenant_id) = Uuid::parse_str(trimmed) {
            let _ = self.inner.allocate_tenant_epoch(tenant_id);
            if let Some(entry) = self.inner.pending_l2_deltas.get(&tenant_id) {
                entry.delta.store(0, Ordering::SeqCst);
            }
            self.inner.l1.invalidate(&tenant_id);
            self.inner.last_l3_fetch.invalidate(&tenant_id);
            debug!(%tenant_id, "Evicted tenant L1 balance slot and pending L2 delta via Pub/Sub");
        }
    }

    /// Returns `true` when the last authoritative L3 fetch for `tenant_id` was more than 1s ago
    /// (or has never occurred), allowing a singleflight L3 recheck before rejecting with `402`.
    #[must_use]
    pub fn should_recheck_l3(&self, tenant_id: Uuid) -> bool {
        self.inner
            .last_l3_fetch
            .get(&tenant_id)
            .is_none_or(|fetched_at| fetched_at.elapsed() > L3_RECHECK_MIN_INTERVAL)
    }

    #[must_use]
    pub fn acquire_l3_recheck_gate(&self, tenant_id: Uuid) -> Arc<tokio::sync::Mutex<()>> {
        Arc::clone(
            self.inner
                .l3_recheck_locks
                .entry(tenant_id)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .value(),
        )
    }

    pub fn release_l3_recheck_gate(&self, tenant_id: Uuid) {
        self.inner
            .l3_recheck_locks
            .remove_if(&tenant_id, |_, gate| Arc::strong_count(gate) <= 2);
    }

    /// 返回缓存中的 `available_balance`（若有）。
    pub async fn get_available(&self, tenant_id: Uuid) -> Option<(Decimal, BalanceCacheSource)> {
        if !self.inner.enabled {
            return None;
        }
        if let Some(slot) = self.inner.l1.get(&tenant_id) {
            let micros = slot.micros.load(Ordering::Acquire);
            let available = micros_to_decimal(micros);
            debug!(%tenant_id, %available, source = "l1", "balance precheck cache hit");
            return Some((available, BalanceCacheSource::L1));
        }
        let key = self.inner.redis_key(tenant_id);
        match self.inner.redis.get_bytes(&key).await {
            Ok(Some(bytes)) => {
                let text = String::from_utf8_lossy(&bytes);
                let parsed_micros = text
                    .trim()
                    .parse::<i64>()
                    .ok()
                    .or_else(|| Decimal::from_str(text.trim()).ok().map(decimal_to_micros));
                if let Some(l2_micros) = parsed_micros {
                    let unflushed = self.inner.pending_delta(tenant_id);
                    let effective_micros = l2_micros.saturating_add(unflushed);
                    let epoch = self.inner.allocate_tenant_epoch(tenant_id);
                    self.inner.l1.insert(
                        tenant_id,
                        Arc::new(L1BalanceSlot {
                            micros: AtomicI64::new(effective_micros),
                            last_l2_micros: AtomicI64::new(l2_micros),
                            epoch,
                        }),
                    );
                    let available = micros_to_decimal(effective_micros);
                    debug!(%tenant_id, %available, source = "l2", "balance precheck redis hit");
                    Some((available, BalanceCacheSource::L2))
                } else {
                    warn!(%tenant_id, "invalid balance cache value in redis");
                    let _ = self.inner.redis.del(&key).await;
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
        if !self.inner.enabled {
            return;
        }
        let micros = decimal_to_micros(available);
        let epoch = self.inner.allocate_tenant_epoch(tenant_id);
        if let Some(entry) = self.inner.pending_l2_deltas.get(&tenant_id) {
            entry.delta.store(0, Ordering::SeqCst);
        }
        self.inner.l1.insert(
            tenant_id,
            Arc::new(L1BalanceSlot {
                micros: AtomicI64::new(micros),
                last_l2_micros: AtomicI64::new(micros),
                epoch,
            }),
        );
        self.inner.last_l3_fetch.insert(tenant_id, Instant::now());
        let key = self.inner.redis_key(tenant_id);
        let value = micros.to_string();
        if let Err(error) = self
            .inner
            .redis
            .set_ex_bytes(&key, value.as_bytes(), self.inner.l2_ttl_secs)
            .await
        {
            warn!(%tenant_id, error = %error, "balance precheck redis set failed");
        }
    }

    /// 请求前单次本地原子预占（`0 ms` 无网络 IO）：扣减 L1 槽位并排队后台 `25 ms` 微批同步 L2。
    pub fn reserve(&self, tenant_id: Uuid, estimated: Decimal) {
        if !self.inner.enabled || estimated <= Decimal::ZERO {
            return;
        }
        let debit_micros = decimal_to_micros(estimated);
        if debit_micros <= 0 {
            return;
        }
        self.inner.record_delta(tenant_id, -debit_micros);
    }

    /// 非计费响应（4xx/5xx/缓存命中/幂等重放/异常提前中止）全额归还请求前预占额度，保留既有 TTL。
    pub fn release(&self, tenant_id: Uuid, reserved: Decimal) {
        self.settle(tenant_id, reserved, Decimal::ZERO);
    }

    /// 响应后差额结算（纯内存完成，后台微批同步 L2）：
    /// - `actual_billable == reserved`：预占已精确扣减，零额外操作（彻底消除双扣！）；
    /// - `actual_billable < reserved`：归还差额 `reserved - actual_billable`；
    /// - `actual_billable > reserved`：补扣差额 `actual_billable - reserved`。
    pub fn settle(&self, tenant_id: Uuid, reserved: Decimal, actual_billable: Decimal) {
        if !self.inner.enabled {
            return;
        }
        let reserved_micros = decimal_to_micros(reserved.max(Decimal::ZERO));
        let actual_micros = decimal_to_micros(actual_billable.max(Decimal::ZERO));
        let delta_micros = reserved_micros.saturating_sub(actual_micros);
        if delta_micros != 0 {
            self.inner.record_delta(tenant_id, delta_micros);
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
            assert_eq!(
                micros_to_decimal(decimal_to_micros(dec)).normalize(),
                dec.normalize()
            );
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
        cache.reserve(tenant_a, est);
        cache.settle(tenant_a, est, est);
        let (after_billable, _) = cache.get_available(tenant_a).await.expect("hit l1");
        assert_eq!(after_billable, Decimal::from_str_exact("0.900000").unwrap());

        // Non-billable 4xx/5xx/replay: reserve(0.1) + release(0.1) => balance fully restored!
        cache.reserve(tenant_a, est);
        cache.release(tenant_a, est);
        let (after_release, _) = cache.get_available(tenant_a).await.expect("hit l1");
        assert_eq!(after_release, Decimal::from_str_exact("0.900000").unwrap());

        // Net-zero tenant_b reserve + release in same window skips Redis batch completely.
        cache.reserve(tenant_b, est);
        cache.release(tenant_b, est);
        let batches = cache.inner.drain_dirty_batches();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 1);
        assert_eq!(batches[0][0].tenant_id, tenant_a);
        assert_eq!(batches[0][0].delta, -100_000);

        // Targeted Pub/Sub invalidation evicts ONLY tenant_a and clears its pending delta.
        cache.reserve(tenant_a, est);
        cache.apply_invalidation_payload(&tenant_a.to_string());
        assert_eq!(cache.inner.pending_delta(tenant_a), 0);
        assert!(cache.get_available(tenant_a).await.is_none());
        assert!(cache.get_available(tenant_b).await.is_some());
    }

    #[tokio::test]
    async fn l2_batch_reconciles_multi_replica_drift_and_fences_stale_epochs() {
        let cache = disconnected_cache();
        let tenant = Uuid::new_v4();
        cache
            .put_available(tenant, Decimal::from_str_exact("1.000000").unwrap())
            .await;

        // First local reserve (-0.10) drained for L2 flush.
        cache.reserve(tenant, Decimal::from_str_exact("0.100000").unwrap());
        let batches = cache.inner.drain_dirty_batches();
        assert_eq!(batches.len(), 1);

        // Concurrent local reserve (-0.05) happens while L2 pipeline is in flight.
        cache.reserve(tenant, Decimal::from_str_exact("0.050000").unwrap());

        // Another replica also deducted 0.20 on Redis, so Redis returns 0.70 (700_000) instead of 0.90.
        cache
            .inner
            .apply_l2_batch_results(&batches[0], vec![Some(700_000)]);
        let (reconciled, _) = cache.get_available(tenant).await.expect("hit l1");
        // Expected L1: 0.70 (L2) - 0.05 (local in-flight) = 0.65.
        assert_eq!(reconciled, Decimal::from_str_exact("0.650000").unwrap());

        // Drain the in-flight -0.05 batch, then simulate authoritative L3 refresh (bumps epoch) before L2 reply.
        let stale_batch = cache.inner.drain_dirty_batches();
        cache
            .put_available(tenant, Decimal::from_str_exact("10.000000").unwrap())
            .await;
        cache
            .inner
            .apply_l2_batch_results(&stale_batch[0], vec![Some(100_000)]);
        let (fresh, _) = cache.get_available(tenant).await.expect("hit fresh l1");
        assert_eq!(fresh, Decimal::from_str_exact("10.000000").unwrap());

        // When L2 TTL expires (`None`), current-epoch L1 slot is evicted to force L3 re-read.
        cache.reserve(tenant, Decimal::from_str_exact("0.100000").unwrap());
        let exp_batch = cache.inner.drain_dirty_batches();
        cache.inner.apply_l2_batch_results(&exp_batch[0], vec![None]);
        assert!(cache.get_available(tenant).await.is_none());
    }
}
