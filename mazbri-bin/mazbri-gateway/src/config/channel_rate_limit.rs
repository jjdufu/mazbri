use serde::Deserialize;
use std::fmt;

/// How channel total QPS is enforced when the token bucket is empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChannelRateLimitMode {
    /// Immediate 429 when no token (default, protects gateway/upstream).
    #[default]
    Reject,
    /// Wait for a token up to max_wait / queue depth guards.
    Queue,
}

impl ChannelRateLimitMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reject => "reject",
            Self::Queue => "queue",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "reject" => Some(Self::Reject),
            "queue" => Some(Self::Queue),
            _ => None,
        }
    }
}

impl fmt::Display for ChannelRateLimitMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Gateway enforcement of `channels.qps_limit` (cross-tenant total capacity).
///
/// Limit values come from the platform route snapshot (`channels.qps_limit`).
/// Per-channel overrides may be set in `channels.config`:
/// - `rate_limit_mode`: `"reject"` | `"queue"`
/// - `rate_limit_max_wait_ms`: u64 (queue only)
/// - `rate_limit_max_queue_depth`: u64 (queue only)
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelRateLimitConfig {
    /// When true, enforce channel QPS before upstream attempts.
    /// Independent of tenant ratelimit `enabled`.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Default mode when channel config does not override.
    #[serde(default)]
    pub default_mode: ChannelRateLimitMode,
    /// Default max wait for queue mode (milliseconds).
    #[serde(default = "default_max_wait_ms")]
    pub max_wait_ms: u64,
    /// Default max concurrent waiters per channel in queue mode.
    #[serde(default = "default_max_queue_depth")]
    pub max_queue_depth: u64,
    /// Bucket capacity = max(1, ceil(qps_limit * burst_multiplier)).
    #[serde(default = "default_burst_multiplier")]
    pub burst_multiplier: f64,
    /// Drop idle channel buckets after this many seconds.
    #[serde(default = "default_bucket_idle_ttl_secs")]
    pub bucket_idle_ttl_secs: u64,
}

fn default_true() -> bool {
    true
}

fn default_burst_multiplier() -> f64 {
    1.0
}

fn default_bucket_idle_ttl_secs() -> u64 {
    600
}

fn default_max_wait_ms() -> u64 {
    1000
}

fn default_max_queue_depth() -> u64 {
    200
}

impl Default for ChannelRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            default_mode: ChannelRateLimitMode::Reject,
            max_wait_ms: default_max_wait_ms(),
            max_queue_depth: default_max_queue_depth(),
            burst_multiplier: default_burst_multiplier(),
            bucket_idle_ttl_secs: default_bucket_idle_ttl_secs(),
        }
    }
}

impl ChannelRateLimitConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if !self.burst_multiplier.is_finite() || self.burst_multiplier <= 0.0 {
            anyhow::bail!(
                "channel_rate_limit.burst_multiplier must be a finite number > 0 (got {})",
                self.burst_multiplier
            );
        }
        if self.bucket_idle_ttl_secs == 0 {
            anyhow::bail!("channel_rate_limit.bucket_idle_ttl_secs must be greater than 0");
        }
        if self.max_wait_ms == 0 {
            anyhow::bail!("channel_rate_limit.max_wait_ms must be greater than 0");
        }
        if self.max_queue_depth == 0 {
            anyhow::bail!("channel_rate_limit.max_queue_depth must be greater than 0");
        }
        Ok(())
    }

    pub fn capacity_for_qps(&self, qps_limit: u64) -> u64 {
        let raw = (qps_limit as f64) * self.burst_multiplier;
        raw.ceil().max(1.0) as u64
    }
}
