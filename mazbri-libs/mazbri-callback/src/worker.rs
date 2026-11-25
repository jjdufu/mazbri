use crate::repository::{self, CallbackDeliveryLog, CallbackSubscription};
use crate::retry::calculate_next_retry;
use crate::signer::{generate_signature, validate_timestamp};
use chrono::Utc;
use mazbri_core::net::ssrf::{SsrfChecker, build_safe_http_client, extract_ssrf_error};
use mazbri_core::num::SaturatingFrom;
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

const CALLBACK_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CALLBACK_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct CallbackWorker {
    pool: PgPool,
    client: Client,
    ssrf: SsrfChecker,
    config: CallbackWorkerConfig,
}

impl CallbackWorker {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self::with_config(pool, &CallbackWorkerConfig::default())
    }

    #[must_use]
    pub fn with_config(pool: PgPool, config: &CallbackWorkerConfig) -> Self {
        Self::with_ssrf(pool, config, SsrfChecker::new())
    }

    #[must_use]
    pub fn with_ssrf(pool: PgPool, config: &CallbackWorkerConfig, ssrf: SsrfChecker) -> Self {
        let client = build_safe_http_client(&ssrf, CALLBACK_CONNECT_TIMEOUT, CALLBACK_REQUEST_TIMEOUT)
            .unwrap_or_else(|e| {
                error!(
                    error = %e,
                    "Failed to build callback safe HTTP client; retrying minimal safe builder"
                );
                ssrf.configure_http_client(Client::builder())
                    .build()
                    .unwrap_or_default()
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
            ssrf,
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
            poll_interval_ms = u64::saturating_from(self.config.poll_interval.as_millis()),
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

    async fn mark_terminal(
        &self,
        task: &CallbackDeliveryLog,
        status: &str,
        reason: &str,
    ) -> anyhow::Result<()> {
        let updated = repository::update_delivery_status(
            &self.pool,
            repository::DeliveryStatusUpdate {
                id: task.id,
                lease_seq: task.lease_seq,
                status,
                retry_count: task.retry_count,
                next_retry_at: None,
                response_code: None,
                error: Some(reason),
            },
        )
        .await?;
        if !updated {
            warn!(
                callback_id = %task.id,
                lease_seq = task.lease_seq,
                "Callback terminal status update fenced out by newer lease"
            );
        }
        Ok(())
    }

    async fn fetch_active_subscription(
        &self,
        task: &CallbackDeliveryLog,
    ) -> anyhow::Result<Option<CallbackSubscription>> {
        let sub_row = sqlx::query_as::<_, CallbackSubscription>(
            "SELECT id, tenant_id, product_id, callback_url, secret_key, retry_policy, is_active FROM mazbri_platform.callback_subscriptions WHERE id = $1",
        )
        .bind(task.subscription_id)
        .fetch_optional(&self.pool)
        .await?;
        match sub_row {
            Some(s) if s.is_active => Ok(Some(s)),
            _ => {
                warn!(
                    "Subscription {} not found or inactive for task {}",
                    task.subscription_id, task.id
                );
                self.mark_terminal(task, "abandoned", "Subscription inactive")
                    .await?;
                Ok(None)
            }
        }
    }

    async fn process_task(&self, task: CallbackDeliveryLog) -> anyhow::Result<()> {
        let Some(sub) = self.fetch_active_subscription(&task).await? else {
            return Ok(());
        };
        let req_builder = match self.ssrf.checked_post(&self.client, &sub.callback_url) {
            Ok(builder) => builder,
            Err(ssrf_err) => {
                warn!(
                    callback_id = %task.id,
                    subscription_id = %task.subscription_id,
                    error = %ssrf_err,
                    "Callback skipped due to SSRF policy rejection"
                );
                self.mark_terminal(&task, "failed", &ssrf_err.to_string())
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
            self.mark_terminal(&task, "failed", "Invalid signature timestamp")
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
                self.mark_terminal(&task, "failed", "Signature generation failed")
                    .await?;
                return Ok(());
            }
        };
        let response = req_builder
            .header("Content-Type", "application/json")
            .header("X-Mazbri-Signature", signature)
            .header("X-Mazbri-Timestamp", timestamp)
            .json(&task.payload)
            .send()
            .await;
        match response {
            Ok(resp) if resp.status().is_success() => {
                info!("Callback {} delivered successfully", task.id);
                let updated = repository::update_delivery_status(
                    &self.pool,
                    repository::DeliveryStatusUpdate {
                        id: task.id,
                        lease_seq: task.lease_seq,
                        status: "success",
                        retry_count: task.retry_count,
                        next_retry_at: None,
                        response_code: Some(i32::from(resp.status().as_u16())),
                        error: None,
                    },
                )
                .await?;
                if !updated {
                    warn!(
                        callback_id = %task.id,
                        lease_seq = task.lease_seq,
                        "Callback success update fenced out by newer lease"
                    );
                }
            }
            Ok(resp) => {
                let status_code = resp.status().as_u16();
                warn!("Callback {} failed with status {}", task.id, status_code);
                self.handle_failure(task, sub, Some(i32::from(status_code)), None)
                    .await?;
            }
            Err(e) => {
                if let Some(ssrf_msg) = extract_ssrf_error(&e) {
                    warn!(
                        callback_id = %task.id,
                        subscription_id = %task.subscription_id,
                        error = %ssrf_msg,
                        "Callback blocked at connect-time DNS by SSRF policy"
                    );
                    self.mark_terminal(&task, "failed", &ssrf_msg).await?;
                } else {
                    error!("Callback {} request error: {}", task.id, e);
                    self.handle_failure(task, sub, None, Some(&e.to_string()))
                        .await?;
                }
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
        let max_retries =
            i32::saturating_from(sub.retry_policy["max_retries"].as_u64().unwrap_or(5));
        let backoff_multiplier = sub.retry_policy["backoff_multiplier"]
            .as_f64()
            .unwrap_or(2.0);
        if task.retry_count < max_retries {
            let next_retry = calculate_next_retry(
                u32::saturating_from(task.retry_count),
                30,
                backoff_multiplier,
            );
            info!(
                "Scheduling retry for callback {} at {}",
                task.id, next_retry
            );
            let updated = repository::update_delivery_status(
                &self.pool,
                repository::DeliveryStatusUpdate {
                    id: task.id,
                    lease_seq: task.lease_seq,
                    status: "retrying",
                    retry_count: task.retry_count + 1,
                    next_retry_at: Some(next_retry),
                    response_code: status_code,
                    error: error_msg,
                },
            )
            .await?;
            if !updated {
                warn!(
                    callback_id = %task.id,
                    lease_seq = task.lease_seq,
                    "Callback retry update fenced out by newer lease"
                );
            }
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
            let updated = repository::update_delivery_status(
                &self.pool,
                repository::DeliveryStatusUpdate {
                    id: task.id,
                    lease_seq: task.lease_seq,
                    status: "failed",
                    retry_count: task.retry_count,
                    next_retry_at: None,
                    response_code: status_code,
                    error: error_msg,
                },
            )
            .await?;
            if !updated {
                warn!(
                    callback_id = %task.id,
                    lease_seq = task.lease_seq,
                    "Callback failure update fenced out by newer lease"
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn callback_safe_client_blocks_loopback_and_disables_redirects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/\r\nContent-Length: 0\r\n\r\n",
                    )
                    .await;
            }
        });

        let strict_ssrf = SsrfChecker::new();
        let strict_client = build_safe_http_client(
            &strict_ssrf,
            CALLBACK_CONNECT_TIMEOUT,
            CALLBACK_REQUEST_TIMEOUT,
        )
        .expect("strict client");
        assert!(
            strict_ssrf
                .checked_post(&strict_client, &format!("http://{addr}/cb"))
                .is_err()
        );
        let err = strict_ssrf
            .checked_post(
                &strict_client,
                &format!("http://metadata.google.internal:{}/cb", addr.port()),
            )
            .expect_err("metadata host blocked synchronously");
        assert!(err.to_string().contains("SSRF Violation:"));
        let dns_err = strict_client
            .post(format!("http://localhost:{}/cb", addr.port()))
            .send()
            .await
            .expect_err("domain loopback callback must be blocked by SsrfSafeResolver");
        assert!(extract_ssrf_error(&dns_err).is_some());

        let allow_local = build_safe_http_client(
            &SsrfChecker::with_allow_private(true),
            CALLBACK_CONNECT_TIMEOUT,
            CALLBACK_REQUEST_TIMEOUT,
        )
        .expect("allow_local client");
        let resp = allow_local
            .post(format!("http://{addr}/cb"))
            .send()
            .await
            .expect("302 returned without following redirect");
        assert_eq!(resp.status().as_u16(), 302);
    }
}
