use serde::Deserialize;
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BillingMiddlewareConfig {
    #[serde(default = "default_billing_enabled")]
    pub enabled: bool,
    #[serde(default = "default_balance_check_enabled")]
    pub balance_check_enabled: bool,
    #[serde(default = "default_usage_record_enabled")]
    pub usage_record_enabled: bool,
    /// 余额预检 L1+L2 缓存（多副本时 L2=Redis 共享）
    #[serde(default = "default_balance_cache_enabled")]
    pub balance_cache_enabled: bool,
    /// L1 本 Pod 内存 TTL（毫秒）
    #[serde(default = "default_balance_cache_l1_ttl_ms")]
    pub balance_cache_l1_ttl_ms: u64,
    /// L2 Redis TTL（秒，最小 1）
    #[serde(default = "default_balance_cache_l2_ttl_secs")]
    pub balance_cache_l2_ttl_secs: u64,
    /// Hard cap for balance precheck (including L3 DB). Never block the proxy for tens of seconds.
    #[serde(default = "default_balance_check_timeout_ms")]
    pub balance_check_timeout_ms: u64,
    /// On precheck timeout / pool / billing infra error: allow the request (true) or 503 (false).
    /// Insufficient balance still returns 402 either way.
    #[serde(default = "default_balance_check_fail_open")]
    pub balance_check_fail_open: bool,
}
fn default_billing_enabled() -> bool {
    true
}
fn default_balance_check_enabled() -> bool {
    true
}
fn default_usage_record_enabled() -> bool {
    true
}
fn default_balance_cache_enabled() -> bool {
    true
}
fn default_balance_cache_l1_ttl_ms() -> u64 {
    // Recommended range 2000–10000 ms:
    // - lower → fresher balance, more L2/DB pressure
    // - higher → larger “stale allow” window after spend (fail-open still applies on infra errors)
    // Default 5s balances burst absorption vs overspend window for typical gateway QPS.
    5000
}
fn default_balance_cache_l2_ttl_secs() -> u64 {
    // Keep L2 ≥ L1; multi-replica sharing. Typical 5–15s.
    10
}
fn default_balance_check_timeout_ms() -> u64 {
    500
}
fn default_balance_check_fail_open() -> bool {
    true
}
impl Default for BillingMiddlewareConfig {
    fn default() -> Self {
        Self {
            enabled: default_billing_enabled(),
            balance_check_enabled: default_balance_check_enabled(),
            usage_record_enabled: default_usage_record_enabled(),
            balance_cache_enabled: default_balance_cache_enabled(),
            balance_cache_l1_ttl_ms: default_balance_cache_l1_ttl_ms(),
            balance_cache_l2_ttl_secs: default_balance_cache_l2_ttl_secs(),
            balance_check_timeout_ms: default_balance_check_timeout_ms(),
            balance_check_fail_open: default_balance_check_fail_open(),
        }
    }
}
