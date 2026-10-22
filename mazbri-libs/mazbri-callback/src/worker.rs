use crate::repository::{self, CallbackDeliveryLog, CallbackSubscription};
use crate::retry::calculate_next_retry;
use crate::signer::{generate_signature, validate_timestamp};
use chrono::Utc;
use reqwest::Client;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{error, info, warn};

const SIGNATURE_TIMESTAMP_MAX_DRIFT_SECS: i64 = 300;

/// Tunables for callback delivery workers.
#[derive(Debug, Clone)]
pub struct CallbackWorkerConfig {
    pub batch_size: i64,
    pub poll_interval: Duration,
    /// Number of independent poll loops (each uses `FOR UPDATE SKIP LOCKED`).
    pub worker_count: usize,
    /// Max concurrent HTTP deliveries **per worker** (bounded semaphore).
    pub max_concurrent_deliveries: usize,
}

impl Default for CallbackWorkerConfig {
    fn default() -> Self {
        Self {
            batch_size: 10,
            poll_interval: Duration::from_secs(5),
            worker_count: 1,
            max_concurrent_deliveries: 2,
        }
    }
}

#[derive(Clone)]
pub struct CallbackWorker {
    pool: PgPool,
    client: Client,
    config: CallbackWorkerConfig,
}

impl CallbackWorker {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self::with_config(pool, CallbackWorkerConfig::default())
    }

    #[must_use]
    pub fn with_config(pool: PgPool, config: CallbackWorkerConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|e| {
                warn!(
                    error = %e,
                    "Failed to build callback HTTP client with timeout; using default client"
                );
                Client::new()
            });
        let config = CallbackWorkerConfig {
            batch_size: config.batch_size.max(1),
            poll_interval: config.poll_interval,
            worker_count: config.worker_count.max(1),
            max_concurrent_deliveries: config.max_concurrent_deliveries.max(1),
        };
        Self {
            pool,
            client,
            config,
        }
    }

    /// Run `worker_count` poll loops until the process exits.
    pub async fn run(&self) {
        let worker_count = self.config.worker_count;
        info!(
            worker_count,
            batch_size = self.config.batch_size,
            max_concurrent = self.config.max_concurrent_deliveries,
            poll_interval_ms = self.config.poll_interval.as_millis() as u64,
            "Starting CallbackWorker fleet"
        );
        if worker_count == 1 {
            self.run_one(0).await;
            return;
        }
        let mut set = JoinSet::new();
        for worker_id in 0..worker_count {
            let worker = self.clone();
            set.spawn(async move {
                worker.run_one(worker_id).await;
            });
        }
        // Fleet runs until process shutdown; join if a worker task panics/exits.
        while let Some(res) = set.join_next().await {
            if let Err(e) = res {
                error!(error = %e, "CallbackWorker task ended unexpectedly");
            }
        }
    }

    async fn run_one(&self, worker_id: usize) {
        info!(worker_id, "CallbackWorker loop started");
        let mut interval = tokio::time::interval(self.config.poll_interval);
        loop {
            interval.tick().await;
            if let Err(e) = self.process_batch(worker_id).await {
                error!(worker_id, "Error processing callback batch: {}", e);
            }
        }
    }

    async fn process_batch(&self, worker_id: usize) -> anyhow::Result<()> {
        let tasks = repository::fetch_pending_tasks(&self.pool, self.config.batch_size).await?;
        if tasks.is_empty() {
            return Ok(());
        }
        info!(
            worker_id,
            count = tasks.len(),
            "Processing pending callbacks"
        );

        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent_deliveries));
        let mut set = JoinSet::new();
        for task in tasks {
            let permit = semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| anyhow::anyhow!("callback delivery semaphore closed: {e}"))?;
            let worker = self.clone();
            set.spawn(async move {
                let _permit = permit;
                worker.process_task(task).await
            });
        }
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    error!(worker_id, "Callback task processing error: {}", e);
                }
                Err(e) => {
                    error!(worker_id, "Callback task join error: {}", e);
                }
            }
        }
        Ok(())
    }

    async fn process_task(&self, task: CallbackDeliveryLog) -> anyhow::Result<()> {
        let sub_row = sqlx::query_as::<_, CallbackSubscription>(
            "SELECT id, tenant_id, product_id, callback_url, secret_key, retry_policy, is_active FROM mazbri_platform.callback_subscriptions WHERE id = $1",
        )
        .bind(task.subscription_id)
        .fetch_optional(&self.pool)
        .await?;
        let sub = match sub_row {
            Some(s) if s.is_active => s,
            _ => {
                warn!(
                    "Subscription {} not found or inactive for task {}",
                    task.subscription_id, task.id
                );
                repository::update_delivery_status(
                    &self.pool,
                    task.id,
                    "abandoned",
                    task.retry_count,
                    None,
                    None,
                    Some("Subscription inactive"),
                )
                .await?;
                return Ok(());
            }
        };
        let timestamp = Utc::now().timestamp();
        if let Err(e) = validate_timestamp(timestamp, SIGNATURE_TIMESTAMP_MAX_DRIFT_SECS) {
            warn!(
                "Callback {} skipped: timestamp validation failed: {}",
                task.id, e
            );
            repository::update_delivery_status(
                &self.pool,
                task.id,
                "failed",
                task.retry_count,
                None,
                None,
                Some("Invalid signature timestamp"),
            )
            .await?;
            return Ok(());
        }
        let payload_str = serde_json::to_string(&task.payload)?;
        let signature = match generate_signature(&payload_str, &sub.secret_key, timestamp) {
            Ok(sig) => sig,
            Err(e) => {
                warn!(
                    "Callback {} skipped: signature generation failed: {}",
                    task.id, e
                );
                repository::update_delivery_status(
                    &self.pool,
                    task.id,
                    "failed",
                    task.retry_count,
                    None,
                    None,
                    Some("Signature generation failed"),
                )
                .await?;
                return Ok(());
            }
        };
        let response = self
            .client
            .post(&sub.callback_url)
            .header("Content-Type", "application/json")
            .header("X-Mazbri-Signature", signature)
            .header("X-Mazbri-Timestamp", timestamp)
            .json(&task.payload)
            .send()
            .await;
        match response {
            Ok(resp) if resp.status().is_success() => {
                info!("Callback {} delivered successfully", task.id);
                repository::update_delivery_status(
                    &self.pool,
                    task.id,
                    "success",
                    task.retry_count,
                    None,
                    Some(i32::from(resp.status().as_u16())),
                    None,
                )
                .await?;
            }
            Ok(resp) => {
                let status_code = resp.status().as_u16();
                warn!("Callback {} failed with status {}", task.id, status_code);
                self.handle_failure(task, sub, Some(i32::from(status_code)), None)
                    .await?;
            }
            Err(e) => {
                error!("Callback {} request error: {}", task.id, e);
                self.handle_failure(task, sub, None, Some(&e.to_string()))
                    .await?;
            }
        }
        Ok(())
    }

    async fn handle_failure(
        &self,
        task: CallbackDeliveryLog,
        sub: CallbackSubscription,
        status_code: Option<i32>,
        error_msg: Option<&str>,
    ) -> anyhow::Result<()> {
        let max_retries = sub.retry_policy["max_retries"].as_u64().unwrap_or(5) as i32;
        let backoff_multiplier = sub.retry_policy["backoff_multiplier"]
            .as_f64()
            .unwrap_or(2.0);
        if task.retry_count < max_retries {
            let next_retry = calculate_next_retry(task.retry_count as u32, 30, backoff_multiplier);
            info!(
                "Scheduling retry for callback {} at {}",
                task.id, next_retry
            );
            repository::update_delivery_status(
                &self.pool,
                task.id,
                "retrying",
                task.retry_count + 1,
                Some(next_retry),
                status_code,
                error_msg,
            )
            .await?;
        } else {
            // Terminal failure: delivery log status=failed is the operator signal (H-25).
            // Poll delivery_logs WHERE status='failed' or subscribe ops webhook separately.
            error!(
                callback_id = %task.id,
                subscription_id = %task.subscription_id,
                retry_count = task.retry_count,
                status_code = ?status_code,
                error = ?error_msg,
                "Callback delivery permanently failed after max retries (ops: check delivery_logs)"
            );
            repository::update_delivery_status(
                &self.pool,
                task.id,
                "failed",
                task.retry_count,
                None,
                status_code,
                error_msg,
            )
            .await?;
        }
        Ok(())
    }
}
