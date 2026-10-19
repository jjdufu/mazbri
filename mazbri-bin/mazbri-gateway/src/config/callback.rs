use serde::Deserialize;
use std::time::Duration;

/// Background callback delivery fleet (mazbri-callback worker).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallbackWorkerTomlConfig {
    #[serde(default = "default_batch_size")]
    pub batch_size: i64,
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,
    /// Independent poll loops; claims via `FOR UPDATE SKIP LOCKED`.
    #[serde(default = "default_worker_count")]
    pub worker_count: usize,
    /// Max concurrent HTTP deliveries per worker.
    #[serde(default = "default_max_concurrent_deliveries")]
    pub max_concurrent_deliveries: usize,
}

fn default_batch_size() -> i64 {
    10
}
fn default_poll_interval_secs() -> u64 {
    5
}
fn default_worker_count() -> usize {
    // One poll loop. Two loops plus the old per-worker delivery cap asked the
    // platform pool (max 8) for more connections than it has.
    1
}
fn default_max_concurrent_deliveries() -> usize {
    2
}

impl Default for CallbackWorkerTomlConfig {
    fn default() -> Self {
        Self {
            batch_size: default_batch_size(),
            poll_interval_secs: default_poll_interval_secs(),
            worker_count: default_worker_count(),
            max_concurrent_deliveries: default_max_concurrent_deliveries(),
        }
    }
}

impl CallbackWorkerTomlConfig {
    #[must_use]
    pub fn to_runtime(&self) -> mazbri_callback::CallbackWorkerConfig {
        mazbri_callback::CallbackWorkerConfig {
            batch_size: self.batch_size.max(1),
            poll_interval: Duration::from_secs(self.poll_interval_secs.max(1)),
            worker_count: self.worker_count.max(1),
            max_concurrent_deliveries: self.max_concurrent_deliveries.max(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CallbackWorkerTomlConfig;

    #[test]
    fn omitted_callback_defaults_fit_platform_pool() {
        let config = CallbackWorkerTomlConfig::default();
        assert_eq!(config.worker_count, 1);
        assert_eq!(config.max_concurrent_deliveries, 2);
    }
}
