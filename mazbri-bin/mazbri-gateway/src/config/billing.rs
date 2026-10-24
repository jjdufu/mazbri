use serde::Deserialize;

/// `[billing]`：计费中间件总开关与用量上报。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BillingMiddlewareConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub usage_record_enabled: bool,
    /// `[billing.balance_check]`
    #[serde(default)]
    pub balance_check: BalanceCheckConfig,
    /// `[billing.usage_events]`: L1 durable usage-event sink. `sink = "redpanda"` produces
    /// each billable usage event synchronously (acks=all) before the response is returned;
    /// on produce failure the request falls back to the outbox/deferred-retry path.
    #[serde(default)]
    pub usage_events: mazbri_billing::UsageEventsConfig,
}

/// `[billing.balance_check]`：余额预检。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BalanceCheckConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Hard cap for balance precheck (including L3 DB). Never block the proxy for tens of seconds.
    #[serde(default = "default_balance_check_timeout_ms")]
    pub timeout_ms: u64,
    /// On precheck timeout / pool / billing infra error: allow the request (true) or 503 (false).
    /// Insufficient balance still returns 402 either way.
    #[serde(default = "default_true")]
    pub fail_open: bool,
    /// `[billing.balance_check.cache]`
    #[serde(default)]
    pub cache: BalanceCacheConfig,
}

/// `[billing.balance_check.cache]`：余额预检 L1+L2 缓存（多副本时 L2=Redis 共享）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BalanceCacheConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// L1 本 Pod 内存 TTL（毫秒）
    #[serde(default = "default_balance_cache_l1_ttl_ms")]
    pub l1_ttl_ms: u64,
    /// L2 Redis TTL（秒，最小 1）
    #[serde(default = "default_balance_cache_l2_ttl_secs")]
    pub l2_ttl_secs: u64,
}

fn default_true() -> bool {
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

impl Default for BillingMiddlewareConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            usage_record_enabled: true,
            balance_check: BalanceCheckConfig::default(),
            usage_events: mazbri_billing::UsageEventsConfig::default(),
        }
    }
}

impl Default for BalanceCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_ms: default_balance_check_timeout_ms(),
            fail_open: true,
            cache: BalanceCacheConfig::default(),
        }
    }
}

impl Default for BalanceCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            l1_ttl_ms: default_balance_cache_l1_ttl_ms(),
            l2_ttl_secs: default_balance_cache_l2_ttl_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_tables_parse_and_default() -> Result<(), toml::de::Error> {
        let cfg: BillingMiddlewareConfig = toml::from_str(
            "enabled = true\nusage_record_enabled = false\n\
             [balance_check]\nfail_open = false\n[balance_check.cache]\nl1_ttl_ms = 2000\n",
        )?;
        assert!(cfg.enabled);
        assert!(!cfg.usage_record_enabled);
        assert!(cfg.balance_check.enabled);
        assert!(!cfg.balance_check.fail_open);
        assert_eq!(cfg.balance_check.timeout_ms, 500);
        assert_eq!(cfg.balance_check.cache.l1_ttl_ms, 2000);
        assert_eq!(cfg.balance_check.cache.l2_ttl_secs, 10);
        Ok(())
    }

    #[test]
    fn legacy_flat_keys_rejected() {
        assert!(toml::from_str::<BillingMiddlewareConfig>("balance_check_enabled = true").is_err());
    }
}
