use anyhow::Result;
use mazbri_core::config_loader::{ServiceConfig, ValidateConfig};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BillingServiceConfig {
    pub server: mazbri_core::ServerConfig,
    /// Write pool (charge workers, outbox, mutations).
    pub database: mazbri_core::DatabaseConfig,
    /// Optional dedicated read pool for balance precheck / reports.
    /// When omitted, reads share `database` (not recommended under high charge QPS).
    #[serde(default)]
    pub database_read: Option<mazbri_core::DatabaseConfig>,
    #[serde(default)]
    pub tls: Option<mazbri_tls::TlsConfig>,
    #[serde(default)]
    pub worker: WorkerConfig,
    #[serde(default)]
    pub admin: mazbri_admin::AdminConfig,
}

impl ServiceConfig for BillingServiceConfig {
    fn service_name() -> &'static str {
        "mazbri-billing-service"
    }
    fn default_port() -> u16 {
        8084
    }
    fn server_config(&self) -> &mazbri_core::ServerConfig {
        &self.server
    }
}

impl ValidateConfig for BillingServiceConfig {
    fn validate(&self) -> Result<()> {
        if let Some(ref tls_config) = self.tls {
            tls_config.validate()?;
        }
        self.worker.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerConfig {
    #[serde(default = "default_worker_enabled")]
    pub enabled: bool,
    /// Rows claimed per loop iteration.
    #[serde(default = "default_batch_size")]
    pub batch_size: i32,
    /// Concurrent processors inside one batch.
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
    /// Parallel drain loops per process (multi-replica still uses SKIP LOCKED).
    #[serde(default = "default_worker_loops")]
    pub worker_loops: usize,
    /// Sleep only when the queue is empty.
    #[serde(default = "default_idle_poll_ms")]
    pub idle_poll_ms: u64,
    /// Reclaim rows stuck in processing (crash recovery).
    #[serde(default = "default_stale_processing_secs")]
    pub stale_processing_secs: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: i32,
    #[serde(default = "default_retry_interval_secs")]
    pub retry_interval_secs: i64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            enabled: default_worker_enabled(),
            batch_size: default_batch_size(),
            max_concurrency: default_max_concurrency(),
            worker_loops: default_worker_loops(),
            idle_poll_ms: default_idle_poll_ms(),
            stale_processing_secs: default_stale_processing_secs(),
            max_retries: default_max_retries(),
            retry_interval_secs: default_retry_interval_secs(),
        }
    }
}

impl WorkerConfig {
    fn validate(&self) -> Result<()> {
        if self.batch_size < 1 {
            anyhow::bail!("worker.batch_size must be >= 1");
        }
        if self.max_concurrency < 1 {
            anyhow::bail!("worker.max_concurrency must be >= 1");
        }
        if self.worker_loops < 1 {
            anyhow::bail!("worker.worker_loops must be >= 1");
        }
        if self.idle_poll_ms < 10 {
            anyhow::bail!("worker.idle_poll_ms must be >= 10");
        }
        if self.stale_processing_secs < 30 {
            anyhow::bail!("worker.stale_processing_secs must be >= 30");
        }
        if self.max_retries < 1 {
            anyhow::bail!("worker.max_retries must be >= 1");
        }
        if self.retry_interval_secs < 1 {
            anyhow::bail!("worker.retry_interval_secs must be >= 1");
        }
        Ok(())
    }
}

fn default_worker_enabled() -> bool {
    true
}
fn default_batch_size() -> i32 {
    500
}
fn default_max_concurrency() -> usize {
    // Concurrent *tenant groups*, not individual charge rows.
    // 1 loop × 4, shared with the usage outbox, fits the write pool max of 10.
    4
}
fn default_worker_loops() -> usize {
    1
}
fn default_idle_poll_ms() -> u64 {
    100
}
fn default_stale_processing_secs() -> u64 {
    120
}
fn default_max_retries() -> i32 {
    3
}
fn default_retry_interval_secs() -> i64 {
    60
}

#[cfg(test)]
mod tests {
    use super::WorkerConfig;

    #[test]
    fn omitted_worker_defaults_fit_write_pool() {
        let worker = WorkerConfig::default();
        assert_eq!(worker.worker_loops, 1);
        assert_eq!(worker.max_concurrency, 4);
    }
}
