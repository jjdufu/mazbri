use mazbri_core::num::SaturatingFrom;
use mazbri_rpc::ratelimit::PolicySnapshot as RpcPolicySnapshot;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnforcementMode {
    LocalEnforced,
    DenyAll,
    ObserveOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StalePolicy {
    StaleAllow,
    StaleDeny,
}

#[derive(Debug, Clone, Default)]
pub struct ApiLimitRule {
    pub qps: Option<u64>,
    pub burst: Option<u64>,
}

impl ApiLimitRule {
    pub fn effective_qps(&self, fallback: u64) -> u64 {
        self.qps.unwrap_or(fallback).max(1)
    }
    /// Prefer explicit burst; else bare-number shorthand uses qps; else plan burst.
    pub fn effective_burst(&self, fallback: u64) -> u64 {
        self.burst.or(self.qps).unwrap_or(fallback).max(1)
    }
}

/// Local cache of a tenant rate-limit policy snapshot from ratelimit-service.
pub struct TenantPolicySnapshot {
    pub tenant_id: Uuid,
    pub version: i64,
    pub enabled: bool,
    pub enforcement_mode: EnforcementMode,
    pub stale_policy: StalePolicy,
    pub max_snapshot_age_secs: u64,
    pub tenant_qps: u64,
    pub tenant_burst: u64,
    pub client_ip_qps: u64,
    pub client_ip_burst: u64,
    /// Bit per second. `0` disables bandwidth enforcement on this gateway instance.
    pub bandwidth_bps: i64,
    pub api_limits: HashMap<String, ApiLimitRule>,
    pub fetched_at: Instant,
}

impl TenantPolicySnapshot {
    pub fn from_rpc(snapshot: RpcPolicySnapshot) -> Option<Self> {
        let tenant_id = Uuid::parse_str(&snapshot.tenant_id).ok()?;
        let enforcement_mode = match snapshot.enforcement_mode.as_str() {
            "deny_all" => EnforcementMode::DenyAll,
            "observe_only" => EnforcementMode::ObserveOnly,
            _ => EnforcementMode::LocalEnforced,
        };
        let stale_policy = match snapshot.stale_policy.as_str() {
            "stale_deny" => StalePolicy::StaleDeny,
            _ => StalePolicy::StaleAllow,
        };
        let api_limits = snapshot
            .api_limits
            .into_iter()
            .map(|(key, value)| {
                (
                    key,
                    ApiLimitRule {
                        qps: value.qps,
                        burst: value.burst,
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        Some(Self {
            tenant_id,
            version: snapshot.version,
            enabled: snapshot.enabled,
            enforcement_mode,
            stale_policy,
            max_snapshot_age_secs: u64::saturating_from(snapshot.max_snapshot_age_secs.max(1)),
            tenant_qps: u64::saturating_from(snapshot.tenant_qps.max(1)),
            tenant_burst: u64::saturating_from(snapshot.tenant_burst.max(1)),
            client_ip_qps: u64::saturating_from(snapshot.client_ip_qps.max(1)),
            client_ip_burst: u64::saturating_from(snapshot.client_ip_burst.max(1)),
            bandwidth_bps: snapshot.bandwidth_bps.max(0),
            api_limits,
            fetched_at: Instant::now(),
        })
    }

    pub fn age_ms(&self) -> u64 {
        mazbri_core::num::millis_u64(self.fetched_at.elapsed())
    }

    pub fn is_stale(&self) -> bool {
        self.fetched_at.elapsed().as_secs() > self.max_snapshot_age_secs
    }

    /// Bandwidth enabled when policy publishes a positive bit/s limit.
    pub fn bandwidth_enabled(&self) -> bool {
        self.bandwidth_bps > 0
    }

    /// Byte/s refill rate for the local bandwidth token bucket.
    pub fn bandwidth_bytes_per_sec(&self) -> u64 {
        if self.bandwidth_bps <= 0 {
            return 0;
        }
        u64::saturating_from(self.bandwidth_bps) / 8
    }

    /// Resolve optional per-API override. `None` means no api-dimension enforcement (P2-4).
    /// Match order (productization §4.2):
    /// 1. `product:{product_id}` 2. bare `product_id` 3. `api:{endpoint}` 4. bare endpoint 5. `prefix*`
    pub fn api_rule(&self, product_id: Option<&str>, api_endpoint: &str) -> Option<&ApiLimitRule> {
        if let Some(product_id) = product_id {
            let exact_keys = [
                format!("product:{product_id}"),
                product_id.to_string(),
                format!("api:{api_endpoint}"),
                api_endpoint.to_string(),
            ];
            for key in exact_keys {
                if let Some(rule) = self.api_limits.get(&key) {
                    return Some(rule);
                }
            }
        } else {
            for key in [api_endpoint.to_string(), format!("api:{api_endpoint}")] {
                if let Some(rule) = self.api_limits.get(&key) {
                    return Some(rule);
                }
            }
        }
        for (key, rule) in &self.api_limits {
            if let Some(prefix) = key.strip_suffix('*')
                && api_endpoint.starts_with(prefix)
            {
                return Some(rule);
            }
        }
        None
    }
}

#[derive(Clone)]
pub struct TenantPolicySnapshotStore {
    default_snapshot: Arc<TenantPolicySnapshot>,
    snapshots: Arc<RwLock<HashMap<Uuid, Arc<TenantPolicySnapshot>>>>,
}

impl TenantPolicySnapshotStore {
    pub fn new(default_snapshot: TenantPolicySnapshot) -> Self {
        Self {
            default_snapshot: Arc::new(default_snapshot),
            snapshots: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn default_snapshot(&self) -> Arc<TenantPolicySnapshot> {
        Arc::clone(&self.default_snapshot)
    }

    pub fn get(&self, tenant_id: Uuid) -> Arc<TenantPolicySnapshot> {
        self.snapshots
            .read()
            .get(&tenant_id)
            .cloned()
            .unwrap_or_else(|| self.default_snapshot())
    }

    pub fn replace_all(&self, snapshots: Vec<TenantPolicySnapshot>) {
        let mut guard = self.snapshots.write();
        guard.clear();
        for snapshot in snapshots {
            guard.insert(snapshot.tenant_id, Arc::new(snapshot));
        }
    }

    /// Max age (seconds) among cached tenant snapshots. `0` when empty.
    pub fn max_snapshot_age_seconds(&self) -> f64 {
        let guard = self.snapshots.read();
        if guard.is_empty() {
            return self.default_snapshot.fetched_at.elapsed().as_secs_f64();
        }
        guard
            .values()
            .map(|snapshot| snapshot.fetched_at.elapsed().as_secs_f64())
            .fold(0.0_f64, f64::max)
    }
}
