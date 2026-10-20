use mazbri_core::num::SaturatingFrom;
use super::commands::ControlCommandStore;
use super::metrics::{
    LOCAL_RATELIMIT_CHECK_DURATION_SECONDS, LOCAL_RATELIMIT_DECISIONS_TOTAL,
    LOCAL_RATELIMIT_USAGE_FLUSH_TOTAL,
};
use super::refresher;
use super::snapshot::{
    EnforcementMode, StalePolicy, TenantPolicySnapshot, TenantPolicySnapshotStore,
};
use super::token_bucket::TokenBucket;
use super::usage_buffer::{UsageDeltaBuffer, UsageDeltaRecord};
use crate::config::ServiceEndpoint;
use crate::tarpc_client::TarpcClientPool;
use dashmap::DashMap;
use mazbri_rpc::ratelimit::UsageDelta;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::warn;
use uuid::Uuid;

/// Burst window for bandwidth token bucket (seconds of traffic allowed as burst).
const BANDWIDTH_BURST_SECS: u64 = 1;

struct UsagePush {
    policy_version: i64,
    snapshot_age_ms: u64,
    degraded: bool,
    allowed: bool,
    reason: Option<String>,
    bytes_in: i64,
    bytes_out: i64,
}

#[derive(Debug, Clone)]
pub struct LocalRateLimitInput {
    pub tenant_id: Uuid,
    pub product_id: Option<String>,
    pub api_endpoint: String,
    pub client_ip: String,
    /// Estimated request body bytes (e.g. Content-Length). `0` skips ingress bandwidth pre-debit.
    pub request_bytes: u64,
}

/// Carried from ratelimit middleware to proxy completion for bandwidth settle + usage bytes.
#[derive(Debug, Clone, Copy)]
pub struct BandwidthPrepaid {
    pub policy_version: i64,
    pub snapshot_age_ms: u64,
    pub degraded: bool,
    pub prepaid_request_bytes: u64,
    pub bandwidth_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct TransferRecord {
    pub tenant_id: Uuid,
    pub product_id: Option<String>,
    pub api_endpoint: String,
    pub client_ip: String,
    pub policy_version: i64,
    pub snapshot_age_ms: u64,
    pub degraded: bool,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub prepaid_request_bytes: u64,
    /// When false, bandwidth bucket settle is skipped (policy had no bandwidth limit).
    pub bandwidth_enabled: bool,
}

#[derive(Debug, Clone)]
pub enum LocalRateLimitDecision {
    Allow {
        policy_version: i64,
        snapshot_age_ms: u64,
        degraded: bool,
        prepaid_request_bytes: u64,
        bandwidth_enabled: bool,
    },
    Deny {
        policy_version: i64,
        retry_after_ms: u64,
        reason: String,
        snapshot_age_ms: u64,
        degraded: bool,
    },
}

#[derive(Debug, Clone, Copy)]
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

    fn refresh_if_needed(&mut self, spec: &BucketSpec, now: Instant) {
        if self.spec.capacity != spec.capacity
            || (self.spec.refill_per_sec - spec.refill_per_sec).abs() > 0.0001
        {
            self.spec = *spec;
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

    fn consume_up_to(&mut self, amount: u64, now: Instant) {
        let _ = self.bucket.consume_up_to(amount, now);
        self.last_seen = now;
    }
}

#[derive(Clone)]
pub struct LocalRateLimitEngine {
    gateway_id: Arc<str>,
    snapshots: Arc<TenantPolicySnapshotStore>,
    buckets: Arc<DashMap<String, Arc<Mutex<BucketCell>>>>,
    usage_buffer: UsageDeltaBuffer,
    commands: ControlCommandStore,
    bucket_idle_ttl: Duration,
}

impl LocalRateLimitEngine {
    pub fn new(gateway_id: impl Into<String>) -> Self {
        let default_snapshot = TenantPolicySnapshot {
            id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            version: 0,
            enabled: true,
            enforcement_mode: EnforcementMode::LocalEnforced,
            stale_policy: StalePolicy::StaleAllow,
            max_snapshot_age_secs: 300,
            tenant_qps: 1000,
            tenant_burst: 1000,
            client_ip_qps: 1000,
            client_ip_burst: 1000,
            bandwidth_bps: 0,
            api_limits: std::collections::HashMap::default(),
            fetched_at: Instant::now(),
        };
        Self {
            gateway_id: Arc::from(gateway_id.into()),
            snapshots: Arc::new(TenantPolicySnapshotStore::new(default_snapshot)),
            buckets: Arc::new(DashMap::new()),
            usage_buffer: UsageDeltaBuffer::new(100_000),
            commands: ControlCommandStore::new(),
            bucket_idle_ttl: Duration::from_secs(600),
        }
    }

    #[must_use]
    pub fn usage_buffer(&self) -> &UsageDeltaBuffer {
        &self.usage_buffer
    }

    #[must_use]
    pub fn gateway_id(&self) -> &str {
        &self.gateway_id
    }

    fn bucket_key(scope: &str, tenant_id: Uuid, key: &str) -> String {
        format!("{scope}:{tenant_id}:{key}")
    }

    fn bandwidth_bucket_key(tenant_id: Uuid) -> String {
        Self::bucket_key("bandwidth", tenant_id, "")
    }

    fn load_or_create_bucket(
        &self,
        key: String,
        spec: &BucketSpec,
        now: Instant,
    ) -> Arc<Mutex<BucketCell>> {
        self.buckets
            .entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(BucketCell::new(*spec, now))))
            .clone()
    }

    fn check_bucket_amount(
        &self,
        key: String,
        spec: BucketSpec,
        amount: u64,
        now: Instant,
    ) -> Result<(), u64> {
        let bucket = self.load_or_create_bucket(key, &spec.clone(), now);
        let mut guard = bucket.lock();
        guard.refresh_if_needed(&spec, now);
        guard.allow_or_retry_after(amount, now)
    }

    fn check_bucket(&self, key: String, spec: BucketSpec, now: Instant) -> Result<(), u64> {
        self.check_bucket_amount(key, spec, 1, now)
    }

    fn bandwidth_spec(bytes_per_sec: u64) -> BucketSpec {
        let refill = bytes_per_sec.max(1);
        BucketSpec {
            capacity: refill.saturating_mul(BANDWIDTH_BURST_SECS).max(1),
            refill_per_sec: mazbri_core::num::u64_to_f64(refill),
        }
    }

    fn build_spec(capacity: u64, qps: u64) -> BucketSpec {
        BucketSpec {
            capacity: capacity.max(1),
            refill_per_sec: mazbri_core::num::u64_to_f64(qps.max(1)),
        }
    }

    fn push_usage(&self, input: &LocalRateLimitInput, rec: UsagePush) {
        let delta = UsageDelta {
            gateway_id: self.gateway_id.to_string(),
            tenant_id: input.tenant_id.to_string(),
            policy_version: rec.policy_version,
            product_id: input.product_id.clone().unwrap_or_default(),
            api_endpoint: input.api_endpoint.clone(),
            client_ip: Some(input.client_ip.clone()),
            window_start_unix: chrono::Utc::now().timestamp(),
            window_secs: 1,
            allowed_count: i64::from(rec.allowed),
            denied_count: i64::from(!rec.allowed),
            bytes_in: rec.bytes_in,
            bytes_out: rec.bytes_out,
            snapshot_age_ms: i64::saturating_from(rec.snapshot_age_ms),
            degraded: rec.degraded,
            degrade_reason: rec.reason,
        };
        self.usage_buffer.record(delta);
    }

    fn observe_only(snapshot: &TenantPolicySnapshot) -> bool {
        matches!(snapshot.enforcement_mode, EnforcementMode::ObserveOnly)
    }

    fn record_decision(decision: &str, degraded: bool, reason: &str, started_at: Instant) {
        let degraded_label = if degraded { "true" } else { "false" };
        LOCAL_RATELIMIT_DECISIONS_TOTAL
            .with_label_values(&[decision, "local", degraded_label, reason])
            .inc();
        LOCAL_RATELIMIT_CHECK_DURATION_SECONDS
            .with_label_values(&[decision, degraded_label])
            .observe(started_at.elapsed().as_secs_f64());
    }

    #[must_use]
    pub fn check(&self, input: &LocalRateLimitInput) -> LocalRateLimitDecision {
        let started_at = Instant::now();
        let snapshot = self.snapshots.get(input.tenant_id);
        // Touch policy id so snapshot correlation stays live across refresh cycles.
        let snapshot_age_ms = snapshot.age_ms();
        let mut degraded = snapshot.tenant_id.is_nil() || snapshot.is_stale();
        let observe_only = Self::observe_only(&snapshot);
        let now_unix = chrono::Utc::now().timestamp();
        if let Some(deny_until_unix) = self.commands.is_denied(input.tenant_id, now_unix)
            && deny_until_unix > now_unix
        {
            let retry_after_ms = u64::saturating_from((deny_until_unix - now_unix).max(1)) * 1000;
            self.push_usage(
                input,
                UsagePush {
                    policy_version: snapshot.version,
                    snapshot_age_ms,
                    degraded: true,
                    allowed: false,
                    reason: Some("deny_until".to_string()),
                    bytes_in: 0,
                    bytes_out: 0,
                },
            );
            Self::record_decision("deny", true, "deny_until", started_at);
            return LocalRateLimitDecision::Deny {
                policy_version: snapshot.version,
                retry_after_ms,
                reason: "deny_until".to_string(),
                snapshot_age_ms,
                degraded: true,
            };
        }
        let multiplier = self.commands.quota_multiplier(input.tenant_id).max(0.1);
        if snapshot.tenant_id.is_nil() {
            degraded = true;
        }
        if !snapshot.enabled {
            self.push_usage(
                input,
                UsagePush {
                    policy_version: snapshot.version,
                    snapshot_age_ms,
                    degraded,
                    allowed: true,
                    reason: Some("disabled_policy".to_string()),
                    bytes_in: 0,
                    bytes_out: 0,
                },
            );
            Self::record_decision("allow", degraded, "disabled", started_at);
            return LocalRateLimitDecision::Allow {
                policy_version: snapshot.version,
                snapshot_age_ms,
                degraded,
                prepaid_request_bytes: 0,
                bandwidth_enabled: false,
            };
        }
        if snapshot.is_stale() {
            match snapshot.stale_policy {
                StalePolicy::StaleDeny => {
                    self.push_usage(
                        input,
                        UsagePush {
                            policy_version: snapshot.version,
                            snapshot_age_ms,
                            degraded: true,
                            allowed: false,
                            reason: Some("snapshot_stale".to_string()),
                            bytes_in: 0,
                            bytes_out: 0,
                        },
                    );
                    Self::record_decision("deny", true, "snapshot_stale", started_at);
                    return LocalRateLimitDecision::Deny {
                        policy_version: snapshot.version,
                        retry_after_ms: 1000,
                        reason: "snapshot_stale".to_string(),
                        snapshot_age_ms,
                        degraded: true,
                    };
                }
                StalePolicy::StaleAllow => {
                    degraded = true;
                }
            }
        }
        if matches!(snapshot.enforcement_mode, EnforcementMode::DenyAll) {
            self.push_usage(
                input,
                UsagePush {
                    policy_version: snapshot.version,
                    snapshot_age_ms,
                    degraded: true,
                    allowed: false,
                    reason: Some("deny_all".to_string()),
                    bytes_in: 0,
                    bytes_out: 0,
                },
            );
            Self::record_decision("deny", true, "deny_all", started_at);
            return LocalRateLimitDecision::Deny {
                policy_version: snapshot.version,
                retry_after_ms: 1000,
                reason: "deny_all".to_string(),
                snapshot_age_ms,
                degraded: true,
            };
        }

        let multiplier = multiplier.clamp(0.1, 10.0);
        let tenant_qps = mazbri_core::num::f64_to_u64((mazbri_core::num::u64_to_f64(snapshot.tenant_qps) * multiplier).round().max(1.0));
        let tenant_burst = mazbri_core::num::f64_to_u64((mazbri_core::num::u64_to_f64(snapshot.tenant_burst) * multiplier)
            .round()
            .max(1.0));
        let client_ip_qps = mazbri_core::num::f64_to_u64((mazbri_core::num::u64_to_f64(snapshot.client_ip_qps) * multiplier)
            .round()
            .max(1.0));
        let client_ip_burst = mazbri_core::num::f64_to_u64((mazbri_core::num::u64_to_f64(snapshot.client_ip_burst) * multiplier)
            .round()
            .max(1.0));
        // Local engine: plan (tenant) → optional API override → IP → bandwidth.
        // API dimension only when an explicit api_limits rule matches (P2-4).

        let tenant_key = Self::bucket_key("tenant", input.tenant_id, "");
        if let Err(retry_after_ms) = self.check_bucket(
            tenant_key,
            Self::build_spec(tenant_burst, tenant_qps),
            Instant::now(),
        ) {
            self.push_usage(
                input,
                UsagePush {
                    policy_version: snapshot.version,
                    snapshot_age_ms,
                    degraded,
                    allowed: false,
                    reason: Some("tenant_qps".to_string()),
                    bytes_in: 0,
                    bytes_out: 0,
                },
            );
            Self::record_decision("deny", degraded, "tenant_qps", started_at);
            return LocalRateLimitDecision::Deny {
                policy_version: snapshot.version,
                retry_after_ms,
                reason: "tenant_qps".to_string(),
                snapshot_age_ms,
                degraded,
            };
        }

        // Skip api dimension when no matching override — avoids api_qps noise at plan QPS.
        if let Some(rule) = snapshot.api_rule(input.product_id.as_deref(), &input.api_endpoint) {
            let api_qps = mazbri_core::num::f64_to_u64((mazbri_core::num::u64_to_f64(rule.effective_qps(tenant_qps)) * multiplier)
                .round()
                .max(1.0));
            let api_burst = mazbri_core::num::f64_to_u64((mazbri_core::num::u64_to_f64(rule.effective_burst(tenant_burst)) * multiplier)
                .round()
                .max(1.0));
            let api_key = Self::bucket_key(
                "api",
                input.tenant_id,
                &format!(
                    "{}:{}",
                    input.product_id.as_deref().unwrap_or("default"),
                    input.api_endpoint
                ),
            );
            if let Err(retry_after_ms) = self.check_bucket(
                api_key,
                Self::build_spec(api_burst, api_qps),
                Instant::now(),
            ) {
                self.push_usage(
                    input,
                    UsagePush {
                        policy_version: snapshot.version,
                        snapshot_age_ms,
                        degraded,
                        allowed: false,
                        reason: Some("api_qps".to_string()),
                        bytes_in: 0,
                        bytes_out: 0,
                    },
                );
                Self::record_decision("deny", degraded, "api_qps", started_at);
                return LocalRateLimitDecision::Deny {
                    policy_version: snapshot.version,
                    retry_after_ms,
                    reason: "api_qps".to_string(),
                    snapshot_age_ms,
                    degraded,
                };
            }
        }

        let ip_key = Self::bucket_key("ip", input.tenant_id, &input.client_ip);
        if let Err(retry_after_ms) = self.check_bucket(
            ip_key,
            Self::build_spec(client_ip_burst, client_ip_qps),
            Instant::now(),
        ) {
            self.push_usage(
                input,
                UsagePush {
                    policy_version: snapshot.version,
                    snapshot_age_ms,
                    degraded,
                    allowed: false,
                    reason: Some("client_ip_qps".to_string()),
                    bytes_in: 0,
                    bytes_out: 0,
                },
            );
            Self::record_decision("deny", degraded, "client_ip_qps", started_at);
            return LocalRateLimitDecision::Deny {
                policy_version: snapshot.version,
                retry_after_ms,
                reason: "client_ip_qps".to_string(),
                snapshot_age_ms,
                degraded,
            };
        }

        let bandwidth_enabled = snapshot.bandwidth_enabled();
        let mut prepaid_request_bytes = 0u64;
        if bandwidth_enabled {
            let bytes_per_sec = snapshot.bandwidth_bytes_per_sec().max(1);
            let spec = Self::bandwidth_spec(bytes_per_sec);
            // amount > capacity can never pass — try_consume handles this.
            if input.request_bytes > 0 {
                let bw_key = Self::bandwidth_bucket_key(input.tenant_id);
                match self.check_bucket_amount(bw_key, spec, input.request_bytes, Instant::now()) {
                    Ok(()) => {
                        prepaid_request_bytes = input.request_bytes;
                    }
                    Err(retry_after_ms) => {
                        if observe_only {
                            // Soft path: do not block; still surface metric/usage reason.
                            self.push_usage(
                                input,
                                UsagePush {
                                    policy_version: snapshot.version,
                                    snapshot_age_ms,
                                    degraded: true,
                                    allowed: true,
                                    reason: Some("bandwidth_observe".to_string()),
                                    bytes_in: i64::saturating_from(input.request_bytes),
                                    bytes_out: 0,
                                },
                            );
                            Self::record_decision("allow", true, "bandwidth_observe", started_at);
                            return LocalRateLimitDecision::Allow {
                                policy_version: snapshot.version,
                                snapshot_age_ms,
                                degraded: true,
                                prepaid_request_bytes: 0,
                                bandwidth_enabled: true,
                            };
                        }
                        self.push_usage(
                            input,
                            UsagePush {
                                policy_version: snapshot.version,
                                snapshot_age_ms,
                                degraded,
                                allowed: false,
                                reason: Some("bandwidth".to_string()),
                                bytes_in: i64::saturating_from(input.request_bytes),
                                bytes_out: 0,
                            },
                        );
                        Self::record_decision("deny", degraded, "bandwidth", started_at);
                        return LocalRateLimitDecision::Deny {
                            policy_version: snapshot.version,
                            retry_after_ms: retry_after_ms.max(1),
                            reason: "bandwidth".to_string(),
                            snapshot_age_ms,
                            degraded,
                        };
                    }
                }
            }
        }

        // U3: keep QPS allowed_count here; bytes filled by record_transfer at response path.
        self.push_usage(
            input,
            UsagePush {
                policy_version: snapshot.version,
                snapshot_age_ms,
                degraded,
                allowed: true,
                reason: None,
                bytes_in: 0,
                bytes_out: 0,
            },
        );
        Self::record_decision("allow", degraded, "ok", started_at);
        LocalRateLimitDecision::Allow {
            policy_version: snapshot.version,
            snapshot_age_ms,
            degraded,
            prepaid_request_bytes,
            bandwidth_enabled,
        }
    }

    /// Post-response: settle bandwidth bucket (in+out shared) and emit bytes-only usage delta (U3).
    pub fn record_transfer(&self, rec: TransferRecord) {
        if rec.bandwidth_enabled {
            let snapshot = self.snapshots.get(rec.tenant_id);
            // Prefer live snapshot rate; fall back to prepaid flag if policy vanished mid-request.
            let bytes_per_sec = if snapshot.bandwidth_enabled() {
                snapshot.bandwidth_bytes_per_sec().max(1)
            } else {
                1
            };
            let spec = Self::bandwidth_spec(bytes_per_sec);
            let extra_in = rec.bytes_in.saturating_sub(rec.prepaid_request_bytes);
            let debit = extra_in.saturating_add(rec.bytes_out);
            if debit > 0 {
                let key = Self::bandwidth_bucket_key(rec.tenant_id);
                let now = Instant::now();
                let cell = self.load_or_create_bucket(key, &spec.clone(), now);
                let mut guard = cell.lock();
                guard.refresh_if_needed(&spec, now);
                guard.consume_up_to(debit, now);
            }
        }

        // Bytes-only usage row (allowed_count/denied_count = 0) so QPS counts stay single-sourced.
        if rec.bytes_in > 0 || rec.bytes_out > 0 {
            let delta = UsageDelta {
                gateway_id: self.gateway_id.to_string(),
                tenant_id: rec.tenant_id.to_string(),
                policy_version: rec.policy_version,
                product_id: rec.product_id.unwrap_or_default(),
                api_endpoint: rec.api_endpoint,
                client_ip: Some(rec.client_ip),
                window_start_unix: chrono::Utc::now().timestamp(),
                window_secs: 1,
                allowed_count: 0,
                denied_count: 0,
                bytes_in: i64::saturating_from(rec.bytes_in),
                bytes_out: i64::saturating_from(rec.bytes_out),
                snapshot_age_ms: i64::saturating_from(rec.snapshot_age_ms),
                degraded: rec.degraded,
                degrade_reason: None,
            };
            self.usage_buffer.record(delta);
        }
    }

    #[must_use]
    pub fn spawn_background_tasks(
        self: Arc<Self>,
        tarpc_pool: &TarpcClientPool,
        ratelimit_endpoint: &ServiceEndpoint,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        vec![
            refresher::spawn_snapshot_refresher(
                Arc::clone(&self),
                tarpc_pool.clone(),
                ratelimit_endpoint.clone(),
            ),
            refresher::spawn_usage_flusher(
                Arc::clone(&self),
                tarpc_pool.clone(),
                ratelimit_endpoint.clone(),
            ),
            refresher::spawn_command_poller(
                Arc::clone(&self),
                tarpc_pool.clone(),
                ratelimit_endpoint.clone(),
            ),
            refresher::spawn_bucket_janitor(Arc::clone(&self)),
        ]
    }

    pub async fn warm_start(
        self: Arc<Self>,
        tarpc_pool: &TarpcClientPool,
        ratelimit_endpoint: &ServiceEndpoint,
    ) {
        if let Err(error) =
            refresher::refresh_snapshots_once(self, tarpc_pool, ratelimit_endpoint).await
        {
            warn!(error = % error, "Gateway ratelimit warm start snapshot refresh failed");
        }
    }

    #[must_use]
    pub fn flush_usage_buffer(&self) -> Vec<UsageDeltaRecord> {
        self.usage_buffer.drain()
    }

    pub fn apply_usage_flush_result(ingested: usize) {
        LOCAL_RATELIMIT_USAGE_FLUSH_TOTAL
            .with_label_values(&["success"])
            .inc_by(ingested as u64);
    }

    #[must_use]
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
        }
        keys.len()
    }

    pub fn update_control_commands(&self, commands: &[mazbri_rpc::ratelimit::ControlCommand]) {
        // Count applied command types for observability (active set after server filter).
        let mut deny = 0u64;
        let mut quota = 0u64;
        let mut refresh = 0u64;
        for command in commands {
            match command.command_type.as_str() {
                "deny_until" => deny += 1,
                "quota_correction" => quota += 1,
                "refresh_policy" => refresh += 1,
                _ => {}
            }
        }
        self.commands.update_from_commands(commands);
        if deny > 0 {
            super::metrics::LOCAL_RATELIMIT_CONTROL_COMMANDS_APPLIED_TOTAL
                .with_label_values(&["deny_until"])
                .inc_by(deny);
        }
        if quota > 0 {
            super::metrics::LOCAL_RATELIMIT_CONTROL_COMMANDS_APPLIED_TOTAL
                .with_label_values(&["quota_correction"])
                .inc_by(quota);
        }
        if refresh > 0 {
            super::metrics::LOCAL_RATELIMIT_CONTROL_COMMANDS_APPLIED_TOTAL
                .with_label_values(&["refresh_policy"])
                .inc_by(refresh);
        }
    }

    /// Consume pending `refresh_policy` request (full snapshot refresh is fine).
    #[must_use]
    pub fn take_policy_refresh_request(&self) -> bool {
        self.commands.take_policy_refresh_request()
    }

    pub fn update_snapshot_age_metric(&self) {
        super::metrics::LOCAL_RATELIMIT_SNAPSHOT_AGE_SECONDS
            .set(self.snapshots.max_snapshot_age_seconds());
    }

    pub fn replace_snapshots(&self, snapshots: Vec<TenantPolicySnapshot>) {
        self.snapshots.replace_all(snapshots);
        self.update_snapshot_age_metric();
    }

    /// Test/helper: inject a single tenant snapshot into the store.
    #[cfg(test)]
    pub fn replace_snapshots_for_test(&self, snapshots: Vec<TenantPolicySnapshot>) {
        self.replace_snapshots(snapshots);
    }
}

#[cfg(test)]
mod tests {
    use super::super::snapshot::ApiLimitRule;
    use super::*;
    use std::collections::HashMap;

    fn engine() -> LocalRateLimitEngine {
        LocalRateLimitEngine::new("test-gw")
    }

    fn snapshot_with_bandwidth(tenant_id: Uuid, bandwidth_bps: i64) -> TenantPolicySnapshot {
        TenantPolicySnapshot {
            id: Uuid::new_v4(),
            tenant_id,
            version: 1,
            enabled: true,
            enforcement_mode: EnforcementMode::LocalEnforced,
            stale_policy: StalePolicy::StaleAllow,
            max_snapshot_age_secs: 300,
            tenant_qps: 10_000,
            tenant_burst: 10_000,
            client_ip_qps: 10_000,
            client_ip_burst: 10_000,
            bandwidth_bps,
            api_limits: HashMap::new(),
            fetched_at: Instant::now(),
        }
    }

    fn input(tenant_id: Uuid, request_bytes: u64) -> LocalRateLimitInput {
        LocalRateLimitInput {
            tenant_id,
            product_id: Some("prod".into()),
            api_endpoint: "/api".into(),
            client_ip: "1.2.3.4".into(),
            request_bytes,
        }
    }

    #[test]
    fn bandwidth_zero_never_denies_on_bytes() {
        let eng = engine();
        let tenant = Uuid::new_v4();
        eng.replace_snapshots_for_test(vec![snapshot_with_bandwidth(tenant, 0)]);
        // Huge body would exceed any small limit; with 0 bps must allow.
        match eng.check(&input(tenant, 50_000_000)) {
            LocalRateLimitDecision::Allow {
                bandwidth_enabled, ..
            } => assert!(!bandwidth_enabled),
            LocalRateLimitDecision::Deny { reason, .. } => {
                panic!("unexpected deny: {reason}")
            }
        }
    }

    #[test]
    fn bandwidth_denies_when_request_exceeds_burst() {
        let eng = engine();
        let tenant = Uuid::new_v4();
        // 1 Mbps => 125_000 byte/s, burst 1s => capacity 125_000
        eng.replace_snapshots_for_test(vec![snapshot_with_bandwidth(tenant, 1_000_000)]);
        match eng.check(&input(tenant, 200_000)) {
            LocalRateLimitDecision::Deny { reason, .. } => assert_eq!(reason, "bandwidth"),
            LocalRateLimitDecision::Allow { .. } => panic!("expected bandwidth deny"),
        }
    }

    #[test]
    fn bandwidth_allows_within_burst_and_prepaids() {
        let eng = engine();
        let tenant = Uuid::new_v4();
        eng.replace_snapshots_for_test(vec![snapshot_with_bandwidth(tenant, 1_000_000)]);
        match eng.check(&input(tenant, 10_000)) {
            LocalRateLimitDecision::Allow {
                prepaid_request_bytes,
                bandwidth_enabled,
                ..
            } => {
                assert!(bandwidth_enabled);
                assert_eq!(prepaid_request_bytes, 10_000);
            }
            LocalRateLimitDecision::Deny { reason, .. } => {
                panic!("unexpected deny: {reason}")
            }
        }
    }

    #[test]
    fn observe_only_bandwidth_allows() {
        let eng = engine();
        let tenant = Uuid::new_v4();
        let mut snap = snapshot_with_bandwidth(tenant, 1_000_000);
        snap.enforcement_mode = EnforcementMode::ObserveOnly;
        eng.replace_snapshots_for_test(vec![snap]);
        match eng.check(&input(tenant, 200_000)) {
            LocalRateLimitDecision::Allow { degraded, .. } => assert!(degraded),
            LocalRateLimitDecision::Deny { reason, .. } => {
                panic!("observe_only must not deny bandwidth: {reason}")
            }
        }
    }

    #[test]
    fn mbps_conversion_one_mbps() {
        let snap = snapshot_with_bandwidth(Uuid::nil(), 1_000_000);
        assert_eq!(snap.bandwidth_bytes_per_sec(), 125_000);
    }

    fn snapshot_plan(tenant_id: Uuid, tenant_qps: u64, tenant_burst: u64) -> TenantPolicySnapshot {
        TenantPolicySnapshot {
            id: Uuid::new_v4(),
            tenant_id,
            version: 1,
            enabled: true,
            enforcement_mode: EnforcementMode::LocalEnforced,
            stale_policy: StalePolicy::StaleAllow,
            max_snapshot_age_secs: 300,
            tenant_qps,
            tenant_burst,
            client_ip_qps: tenant_qps,
            client_ip_burst: tenant_burst,
            bandwidth_bps: 0,
            api_limits: HashMap::new(),
            fetched_at: Instant::now(),
        }
    }

    /// P2-4: empty `api_limits` must not invent a second api bucket / `api_qps` reason.
    #[test]
    fn empty_api_limits_skips_api_dimension() {
        let eng = engine();
        let tenant = Uuid::new_v4();
        // burst=1,qps=1 so second request is denied by plan, not api.
        eng.replace_snapshots_for_test(vec![snapshot_plan(tenant, 1, 1)]);
        assert!(matches!(
            eng.check(&input(tenant, 0)),
            LocalRateLimitDecision::Allow { .. }
        ));
        match eng.check(&input(tenant, 0)) {
            LocalRateLimitDecision::Deny { reason, .. } => {
                assert_eq!(
                    reason, "tenant_qps",
                    "must not be api_qps without overrides"
                );
            }
            LocalRateLimitDecision::Allow { .. } => panic!("expected tenant_qps deny"),
        }
    }

    /// P2-4: explicit override still enforces `api_qps` when tighter than plan.
    #[test]
    fn matching_api_override_enforces_api_qps() {
        let eng = engine();
        let tenant = Uuid::new_v4();
        let mut snap = snapshot_plan(tenant, 10_000, 10_000);
        snap.api_limits.insert(
            "/api".into(),
            ApiLimitRule {
                qps: Some(1),
                burst: Some(1),
            },
        );
        eng.replace_snapshots_for_test(vec![snap]);
        assert!(matches!(
            eng.check(&input(tenant, 0)),
            LocalRateLimitDecision::Allow { .. }
        ));
        match eng.check(&input(tenant, 0)) {
            LocalRateLimitDecision::Deny { reason, .. } => assert_eq!(reason, "api_qps"),
            LocalRateLimitDecision::Allow { .. } => panic!("expected api_qps deny"),
        }
    }
}
