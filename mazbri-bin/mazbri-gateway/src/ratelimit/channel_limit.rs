//! Cross-tenant channel QPS enforcement (`channels.qps_limit`).
//!
//! - Keyed by `channel_id` only (shared across all tenants on this gateway instance).
//! - Independent of tenant ratelimit policy / `enabled` flag.
//! - `qps_limit <= 0` means unlimited (no token consumed).
//! - Modes: **reject** (default, immediate 429) or **queue** (wait with guards).

use super::metrics::{
    CHANNEL_RATELIMIT_CHECK_DURATION_SECONDS, CHANNEL_RATELIMIT_DECISIONS_TOTAL,
    CHANNEL_RATELIMIT_QUEUE_WAIT_SECONDS,
};
use super::token_bucket::TokenBucket;
use crate::config::{ChannelRateLimitConfig, ChannelRateLimitMode};
use dashmap::DashMap;
use parking_lot::Mutex;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tracing::info;

/// Per-request overrides resolved from `channels.config` (+ global defaults).
#[derive(Debug, Clone)]
pub struct ChannelRateLimitRequest {
    pub channel_id: String,
    pub channel_code: String,
    pub qps_limit: i32,
    /// Full channel JSON config from route snapshot.
    pub channel_config: Value,
}

#[derive(Debug, Clone)]
pub enum ChannelRateLimitDecision {
    Allow {
        /// Time spent waiting in queue mode (0 for reject path / immediate allow).
        queue_wait_ms: u64,
        mode: ChannelRateLimitMode,
    },
    Deny {
        retry_after_ms: u64,
        qps_limit: u64,
        channel_id: String,
        channel_code: String,
        /// `channel_qps` | `queue_timeout` | `queue_full`
        reason: &'static str,
        mode: ChannelRateLimitMode,
    },
}

#[derive(Debug, Clone)]
struct BucketSpec {
    capacity: u64,
    refill_per_sec: f64,
}

#[derive(Debug)]
struct BucketCell {
    spec: BucketSpec,
    bucket: TokenBucket,
    last_seen: Instant,
}

impl BucketCell {
    fn new(spec: BucketSpec, now: Instant) -> Self {
        let bucket = TokenBucket::new(spec.capacity, spec.refill_per_sec, now);
        Self {
            spec,
            bucket,
            last_seen: now,
        }
    }

    fn refresh_if_needed(&mut self, spec: BucketSpec, now: Instant) {
        if self.spec.capacity != spec.capacity
            || (self.spec.refill_per_sec - spec.refill_per_sec).abs() > 0.0001
        {
            self.spec = spec.clone();
            self.bucket.reset(spec.capacity, spec.refill_per_sec, now);
        }
        self.last_seen = now;
    }

    fn allow_or_retry_after(&mut self, amount: u64, now: Instant) -> Result<(), u64> {
        if self.bucket.try_consume(amount, now) {
            Ok(())
        } else {
            Err(self.bucket.retry_after_ms(amount, now))
        }
    }
}

/// Decrements queue depth when dropped (including cancel/timeout paths).
struct QueueSlot {
    depth: Arc<AtomicUsize>,
}

impl Drop for QueueSlot {
    fn drop(&mut self) {
        self.depth.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone)]
pub struct ChannelRateLimitEngine {
    config: ChannelRateLimitConfig,
    buckets: Arc<DashMap<String, Arc<Mutex<BucketCell>>>>,
    queue_depths: Arc<DashMap<String, Arc<AtomicUsize>>>,
    bucket_idle_ttl: Duration,
}

impl ChannelRateLimitEngine {
    pub fn new(config: ChannelRateLimitConfig) -> Self {
        let bucket_idle_ttl = Duration::from_secs(config.bucket_idle_ttl_secs.max(1));
        info!(
            enabled = config.enabled,
            default_mode = %config.default_mode,
            max_wait_ms = config.max_wait_ms,
            max_queue_depth = config.max_queue_depth,
            burst_multiplier = config.burst_multiplier,
            bucket_idle_ttl_secs = bucket_idle_ttl.as_secs(),
            "Channel rate limit engine initialized (cross-tenant, per channel_id)"
        );
        Self {
            config,
            buckets: Arc::new(DashMap::new()),
            queue_depths: Arc::new(DashMap::new()),
            bucket_idle_ttl,
        }
    }

    fn resolve_mode(&self, channel_config: &Value) -> ChannelRateLimitMode {
        channel_config
            .get("rate_limit_mode")
            .and_then(Value::as_str)
            .and_then(ChannelRateLimitMode::parse)
            .unwrap_or(self.config.default_mode)
    }

    fn resolve_max_wait_ms(&self, channel_config: &Value) -> u64 {
        channel_config
            .get("rate_limit_max_wait_ms")
            .and_then(Value::as_u64)
            .filter(|v| *v > 0)
            .unwrap_or(self.config.max_wait_ms)
            .max(1)
    }

    fn resolve_max_queue_depth(&self, channel_config: &Value) -> u64 {
        channel_config
            .get("rate_limit_max_queue_depth")
            .and_then(Value::as_u64)
            .filter(|v| *v > 0)
            .unwrap_or(self.config.max_queue_depth)
            .max(1)
    }

    fn load_or_create_bucket(
        &self,
        channel_id: &str,
        spec: BucketSpec,
        now: Instant,
    ) -> Arc<Mutex<BucketCell>> {
        self.buckets
            .entry(channel_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(BucketCell::new(spec.clone(), now))))
            .clone()
    }

    fn try_consume_once(
        &self,
        channel_id: &str,
        spec: BucketSpec,
        now: Instant,
    ) -> Result<(), u64> {
        let cell = self.load_or_create_bucket(channel_id, spec.clone(), now);
        let mut guard = cell.lock();
        guard.refresh_if_needed(spec, now);
        guard.allow_or_retry_after(1, now)
    }

    fn record_allow(
        &self,
        started_at: Instant,
        mode: ChannelRateLimitMode,
        reason: &str,
        queue_wait_ms: u64,
    ) {
        CHANNEL_RATELIMIT_DECISIONS_TOTAL
            .with_label_values(&["allow", reason, mode.as_str()])
            .inc();
        CHANNEL_RATELIMIT_CHECK_DURATION_SECONDS
            .with_label_values(&["allow", mode.as_str()])
            .observe(started_at.elapsed().as_secs_f64());
        if queue_wait_ms > 0 {
            CHANNEL_RATELIMIT_QUEUE_WAIT_SECONDS
                .with_label_values(&["ok"])
                .observe(queue_wait_ms as f64 / 1000.0);
        }
    }

    fn record_deny(&self, started_at: Instant, mode: ChannelRateLimitMode, reason: &str) {
        CHANNEL_RATELIMIT_DECISIONS_TOTAL
            .with_label_values(&["deny", reason, mode.as_str()])
            .inc();
        CHANNEL_RATELIMIT_CHECK_DURATION_SECONDS
            .with_label_values(&["deny", mode.as_str()])
            .observe(started_at.elapsed().as_secs_f64());
        if reason == "queue_timeout" || reason == "queue_full" {
            CHANNEL_RATELIMIT_QUEUE_WAIT_SECONDS
                .with_label_values(&[reason])
                .observe(started_at.elapsed().as_secs_f64());
        }
    }

    /// Check channel total QPS before an upstream attempt (async: supports queue mode).
    pub async fn check(&self, req: ChannelRateLimitRequest) -> ChannelRateLimitDecision {
        let started_at = Instant::now();
        let mode = self.resolve_mode(&req.channel_config);

        if !self.config.enabled {
            self.record_allow(started_at, mode, "disabled", 0);
            return ChannelRateLimitDecision::Allow {
                queue_wait_ms: 0,
                mode,
            };
        }
        if req.qps_limit <= 0 {
            self.record_allow(started_at, mode, "unlimited", 0);
            return ChannelRateLimitDecision::Allow {
                queue_wait_ms: 0,
                mode,
            };
        }

        let qps = req.qps_limit as u64;
        let capacity = self.config.capacity_for_qps(qps);
        let spec = BucketSpec {
            capacity,
            refill_per_sec: qps as f64,
        };

        // Fast path: consume without waiting.
        match self.try_consume_once(&req.channel_id, spec.clone(), Instant::now()) {
            Ok(()) => {
                self.record_allow(started_at, mode, "channel_qps", 0);
                return ChannelRateLimitDecision::Allow {
                    queue_wait_ms: 0,
                    mode,
                };
            }
            Err(retry_after_ms) => {
                if mode == ChannelRateLimitMode::Reject {
                    self.record_deny(started_at, mode, "channel_qps");
                    return ChannelRateLimitDecision::Deny {
                        retry_after_ms: retry_after_ms.max(1),
                        qps_limit: qps,
                        channel_id: req.channel_id,
                        channel_code: req.channel_code,
                        reason: "channel_qps",
                        mode,
                    };
                }
            }
        }

        // Queue mode: limited wait with depth guard.
        self.check_queue(req, qps, spec, mode, started_at).await
    }

    async fn check_queue(
        &self,
        req: ChannelRateLimitRequest,
        qps: u64,
        spec: BucketSpec,
        mode: ChannelRateLimitMode,
        started_at: Instant,
    ) -> ChannelRateLimitDecision {
        let max_wait = Duration::from_millis(self.resolve_max_wait_ms(&req.channel_config));
        let max_depth = self.resolve_max_queue_depth(&req.channel_config) as usize;

        let depth = self
            .queue_depths
            .entry(req.channel_id.clone())
            .or_insert_with(|| Arc::new(AtomicUsize::new(0)))
            .clone();

        // Reserve a queue slot (depth = waiters currently blocked).
        let prev = depth.fetch_add(1, Ordering::AcqRel);
        if prev >= max_depth {
            depth.fetch_sub(1, Ordering::AcqRel);
            self.record_deny(started_at, mode, "queue_full");
            return ChannelRateLimitDecision::Deny {
                retry_after_ms: 1,
                qps_limit: qps,
                channel_id: req.channel_id,
                channel_code: req.channel_code,
                reason: "queue_full",
                mode,
            };
        }
        let _slot = QueueSlot { depth };

        let deadline = started_at + max_wait;
        loop {
            let now = Instant::now();
            if now >= deadline {
                let retry_after_ms = self
                    .try_consume_once(&req.channel_id, spec.clone(), now)
                    .err()
                    .unwrap_or(1)
                    .max(1);
                self.record_deny(started_at, mode, "queue_timeout");
                return ChannelRateLimitDecision::Deny {
                    retry_after_ms,
                    qps_limit: qps,
                    channel_id: req.channel_id,
                    channel_code: req.channel_code,
                    reason: "queue_timeout",
                    mode,
                };
            }

            match self.try_consume_once(&req.channel_id, spec.clone(), now) {
                Ok(()) => {
                    let queue_wait_ms = started_at.elapsed().as_millis() as u64;
                    self.record_allow(started_at, mode, "channel_qps", queue_wait_ms);
                    return ChannelRateLimitDecision::Allow {
                        queue_wait_ms,
                        mode,
                    };
                }
                Err(retry_after_ms) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        continue;
                    }
                    // Sleep until next expected token, capped by remaining wait and a small poll.
                    let sleep_ms = retry_after_ms
                        .clamp(1, 50)
                        .min(remaining.as_millis() as u64);
                    tokio::time::sleep(Duration::from_millis(sleep_ms.max(1))).await;
                }
            }
        }
    }

    pub fn prune_buckets(&self) -> usize {
        let now = Instant::now();
        let keys = self
            .buckets
            .iter()
            .filter_map(|entry| {
                let guard = entry.value().lock();
                if now.duration_since(guard.last_seen) > self.bucket_idle_ttl {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for key in &keys {
            self.buckets.remove(key);
            // Drop depth counters only when no waiters remain.
            if let Some(entry) = self.queue_depths.get(key)
                && entry.load(Ordering::Acquire) == 0
            {
                drop(entry);
                self.queue_depths.remove(key);
            }
        }
        keys.len()
    }

    pub fn spawn_background_tasks(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let removed = self.prune_buckets();
                if removed > 0 {
                    info!(removed, "Removed stale channel rate-limit buckets");
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn req(id: &str, code: &str, qps: i32, config: Value) -> ChannelRateLimitRequest {
        ChannelRateLimitRequest {
            channel_id: id.to_string(),
            channel_code: code.to_string(),
            qps_limit: qps,
            channel_config: config,
        }
    }

    #[tokio::test]
    async fn unlimited_when_qps_zero() {
        let engine = ChannelRateLimitEngine::new(ChannelRateLimitConfig::default());
        for _ in 0..100 {
            let d = engine.check(req("ch-1", "POSTS", 0, Value::Null)).await;
            assert!(matches!(d, ChannelRateLimitDecision::Allow { .. }));
        }
    }

    #[tokio::test]
    async fn reject_denies_after_capacity_exhausted() {
        let cfg = ChannelRateLimitConfig {
            burst_multiplier: 1.0,
            default_mode: ChannelRateLimitMode::Reject,
            ..Default::default()
        };
        let engine = ChannelRateLimitEngine::new(cfg);
        let mut denies = 0u32;
        for _ in 0..10 {
            if matches!(
                engine.check(req("ch-posts", "POSTS", 5, Value::Null)).await,
                ChannelRateLimitDecision::Deny {
                    reason: "channel_qps",
                    ..
                }
            ) {
                denies += 1;
            }
        }
        assert!(denies >= 5, "expected denies after burst, got {denies}");
    }

    #[tokio::test]
    async fn disabled_always_allows() {
        let cfg = ChannelRateLimitConfig {
            enabled: false,
            ..Default::default()
        };
        let engine = ChannelRateLimitEngine::new(cfg);
        for _ in 0..50 {
            let d = engine.check(req("ch-1", "POSTS", 1, Value::Null)).await;
            assert!(matches!(d, ChannelRateLimitDecision::Allow { .. }));
        }
    }

    #[tokio::test]
    async fn queue_mode_eventually_allows() {
        let cfg = ChannelRateLimitConfig {
            default_mode: ChannelRateLimitMode::Queue,
            max_wait_ms: 2000,
            max_queue_depth: 50,
            burst_multiplier: 1.0,
            ..Default::default()
        };
        let engine = ChannelRateLimitEngine::new(cfg);

        // Exhaust burst (1).
        let first = engine.check(req("ch-q", "Q", 1, Value::Null)).await;
        assert!(matches!(first, ChannelRateLimitDecision::Allow { .. }));

        // Second must wait ~1s for refill then allow.
        let second = engine.check(req("ch-q", "Q", 1, Value::Null)).await;
        match second {
            ChannelRateLimitDecision::Allow {
                queue_wait_ms,
                mode,
            } => {
                assert_eq!(mode, ChannelRateLimitMode::Queue);
                assert!(
                    queue_wait_ms >= 50,
                    "expected some wait, got {queue_wait_ms}"
                );
            }
            other => panic!("expected Allow after queue, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn queue_full_when_depth_exceeded() {
        let cfg = ChannelRateLimitConfig {
            default_mode: ChannelRateLimitMode::Queue,
            max_wait_ms: 5000,
            max_queue_depth: 2,
            burst_multiplier: 1.0,
            ..Default::default()
        };
        let engine = Arc::new(ChannelRateLimitEngine::new(cfg));

        // Consume the only token (qps=1).
        let _ = engine.check(req("ch-full", "F", 1, Value::Null)).await;

        let e1 = Arc::clone(&engine);
        let e2 = Arc::clone(&engine);
        let e3 = Arc::clone(&engine);
        let t1 = tokio::spawn(async move { e1.check(req("ch-full", "F", 1, Value::Null)).await });
        let t2 = tokio::spawn(async move { e2.check(req("ch-full", "F", 1, Value::Null)).await });
        // Give first two time to enter queue.
        tokio::time::sleep(Duration::from_millis(30)).await;
        let third = e3.check(req("ch-full", "F", 1, Value::Null)).await;
        assert!(
            matches!(
                third,
                ChannelRateLimitDecision::Deny {
                    reason: "queue_full",
                    ..
                }
            ),
            "expected queue_full, got {third:?}"
        );
        // Cancel waiters by dropping (abort tasks).
        t1.abort();
        t2.abort();
        let _ = t1.await;
        let _ = t2.await;
    }

    #[tokio::test]
    async fn channel_config_overrides_mode_to_queue() {
        let cfg = ChannelRateLimitConfig {
            default_mode: ChannelRateLimitMode::Reject,
            max_wait_ms: 2000,
            ..Default::default()
        };
        let engine = ChannelRateLimitEngine::new(cfg);

        let _ = engine.check(req("ch-ov", "O", 1, Value::Null)).await;

        let override_cfg = json!({
            "rate_limit_mode": "queue",
            "rate_limit_max_wait_ms": 2000,
            "rate_limit_max_queue_depth": 10
        });
        let d = engine.check(req("ch-ov", "O", 1, override_cfg)).await;
        assert!(
            matches!(
                d,
                ChannelRateLimitDecision::Allow {
                    mode: ChannelRateLimitMode::Queue,
                    ..
                }
            ),
            "channel override should queue, got {d:?}"
        );
    }
}
