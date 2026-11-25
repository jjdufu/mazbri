use crate::cbpro_client::CBProClient;
use crate::config::CBProExtServiceConfig;
use crate::db::CbProExtRepository;
use crate::greptime::GreptimeClient;
use crate::observations::now_unix_ms;
use crate::runtime::{
    RuntimeDecisionState, bounded_validation_cache_key, build_runtime_decision_cache_key,
    canonical_runtime_decision_key,
};
use crate::validation::{build_validation_request, process_validation_job};
use anyhow::Result;
use mazbri_core::num::SaturatingFrom;
use mazbri_rpc::{
    ObservationRequest, ObservationResponse, RuntimeDecisionRequest, RuntimeDecisionResponse,
    ValidationEnqueueRequest, ValidationEnqueueResponse, ValidationResultRequest,
    ValidationResultResponse,
};
use moka::sync::Cache;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{info, warn};

const RUNTIME_STATE_IDLE_SECS: u64 = 600;

pub struct CbProExtManager {
    config: CBProExtServiceConfig,
    repo: Arc<CbProExtRepository>,
    cbpro_client: Option<Arc<CBProClient>>,
    greptime_client: Option<GreptimeClient>,
    runtime_state: Cache<String, Arc<Mutex<RuntimeDecisionState>>>,
}

impl CbProExtManager {
    #[must_use]
    pub fn new(
        config: CBProExtServiceConfig,
        repo: Arc<CbProExtRepository>,
        cbpro_client: Option<Arc<CBProClient>>,
        greptime_client: Option<GreptimeClient>,
    ) -> Self {
        let max_cap = u64::try_from(config.runtime.max_runtime_entries.max(1)).unwrap_or(100_000);
        let runtime_state = Cache::builder()
            .max_capacity(max_cap)
            .time_to_idle(Duration::from_secs(RUNTIME_STATE_IDLE_SECS))
            .build();
        Self {
            config,
            repo,
            cbpro_client,
            greptime_client,
            runtime_state,
        }
    }

    pub async fn record_observation(&self, request: ObservationRequest) -> ObservationResponse {
        let Some(raw_cache_key) = request.cache_key.clone() else {
            return ObservationResponse {
                success: true,
                error: None,
            };
        };
        let runtime_key = canonical_runtime_decision_key(
            &request.tenant_id,
            &request.product_id,
            &request.method,
            &request.path,
            &raw_cache_key,
        );
        let cell = self.runtime_state.get_with_by_ref(&runtime_key, || {
            Arc::new(Mutex::new(RuntimeDecisionState::new(
                &RuntimeDecisionRequest {
                    tenant_id: request.tenant_id.clone(),
                    product_id: request.product_id.clone(),
                    method: request.method.clone(),
                    path: request.path.clone(),
                    cache_key: raw_cache_key.clone(),
                    policy_version: None,
                },
                &self.config.runtime,
                now_unix_ms(),
            )))
        });
        let (decision, validate_due) = {
            let mut state = cell.lock();
            let due = state.observe(
                &request.event_type,
                request.service_total_ms,
                &self.config.runtime,
                now_unix_ms(),
            );
            (state.decision.clone(), due)
        };
        if let Err(error) = self.repo.upsert_runtime_decision(&decision).await {
            warn!(error = %error, cache_key = %runtime_key, "Failed to persist runtime decision");
        }
        if validate_due && self.config.validation.enabled {
            let enqueue_request = build_validation_request(
                bounded_validation_cache_key(&raw_cache_key),
                request.tenant_id.clone(),
                request.product_id.clone(),
                request.method.clone(),
                request.path.clone(),
                format!("event:{}", request.event_type),
            );
            let response = self.enqueue_validation(enqueue_request).await;
            if !response.accepted
                && let Some(error) = response.error
            {
                warn!(error = %error.message(), "Failed to enqueue validation from observation");
            }
        }
        ObservationResponse {
            success: true,
            error: None,
        }
    }

    pub async fn get_runtime_decision(
        &self,
        request: RuntimeDecisionRequest,
    ) -> RuntimeDecisionResponse {
        let key = build_runtime_decision_cache_key(&request);
        if let Some(cell) = self.runtime_state.get(&key) {
            let decision = cell.lock().decision.clone();
            return RuntimeDecisionResponse {
                available: true,
                decision: Some(decision),
                error: None,
            };
        }
        match self.repo.get_runtime_decision(&key).await {
            Ok(Some(decision)) => {
                self.runtime_state.insert(
                    key,
                    Arc::new(Mutex::new(RuntimeDecisionState {
                        baseline_ema_ms: mazbri_core::num::u64_to_f64(decision.latency_baseline_ms),
                        hit_count_since_validate: 0,
                        last_hit_at_unix_ms: 0,
                        last_validated_at_unix_ms: 0,
                        decision: decision.clone(),
                    })),
                );
                RuntimeDecisionResponse {
                    available: true,
                    decision: Some(decision),
                    error: None,
                }
            }
            Ok(None) => {
                let cell = self.runtime_state.get_with_by_ref(&key, || {
                    Arc::new(Mutex::new(RuntimeDecisionState::new(
                        &request,
                        &self.config.runtime,
                        now_unix_ms(),
                    )))
                });
                let decision = cell.lock().decision.clone();
                if let Err(error) = self.repo.upsert_runtime_decision(&decision).await {
                    warn!(error = %error, cache_key = %key, "Failed to seed runtime decision");
                }
                RuntimeDecisionResponse {
                    available: true,
                    decision: Some(decision),
                    error: None,
                }
            }
            Err(error) => RuntimeDecisionResponse {
                available: false,
                decision: None,
                error: Some(mazbri_rpc::RpcError::ServiceUnavailable {
                    service: mazbri_core::ServiceName::CbproExt.as_str().into(),
                    message: error.to_string(),
                }),
            },
        }
    }

    pub async fn enqueue_validation(
        &self,
        mut request: ValidationEnqueueRequest,
    ) -> ValidationEnqueueResponse {
        request.cache_key = bounded_validation_cache_key(&request.cache_key);
        match self.repo.enqueue_validation_job(&request).await {
            Ok(job_id) => ValidationEnqueueResponse {
                accepted: job_id.is_some(),
                job_id: job_id.map(|id| id.to_string()),
                error: None,
            },
            Err(error) => ValidationEnqueueResponse {
                accepted: false,
                job_id: None,
                error: Some(mazbri_rpc::RpcError::ServiceUnavailable {
                    service: mazbri_core::ServiceName::CbproExt.as_str().into(),
                    message: error.to_string(),
                }),
            },
        }
    }

    pub async fn report_validation_result(
        &self,
        request: ValidationResultRequest,
    ) -> ValidationResultResponse {
        let key = canonical_runtime_decision_key(
            &request.tenant_id,
            &request.product_id,
            &request.method,
            &request.path,
            &request.cache_key,
        );
        let decision_to_persist = self.runtime_state.get(&key).map(|cell| {
            let mut state = cell.lock();
            state.mark_validated(request.message.clone(), now_unix_ms());
            state.decision.clone()
        });
        if let Some(decision) = decision_to_persist
            && let Err(error) = self.repo.upsert_runtime_decision(&decision).await
        {
            warn!(error = %error, cache_key = %key, "Failed to persist validation result");
        }
        if let Err(error) = self
            .repo
            .mark_validation_result(
                &request,
                if request.success {
                    "completed"
                } else {
                    "failed"
                },
            )
            .await
        {
            warn!(error = %error, cache_key = %key, "Failed to record validation job result");
        }
        ValidationResultResponse {
            recorded: true,
            error: None,
        }
    }

    /// Claims up to `max_inflight` validation jobs (including stale `'running'` jobs whose
    /// `locked_until` expired) and executes them concurrently up to `worker_count`.
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn process_validation_jobs(&self) -> Result<()> {
        if !self.config.validation.enabled {
            return Ok(());
        }
        let jobs = self
            .repo
            .claim_validation_jobs(
                i64::saturating_from(self.config.validation.max_inflight),
                i64::saturating_from(self.config.validation.lock_ttl_secs),
            )
            .await?;
        if jobs.is_empty() {
            return Ok(());
        }
        let concurrency = self.config.validation.worker_count.max(1);
        let sem = Arc::new(Semaphore::new(concurrency));
        let mut set = JoinSet::new();
        for job in jobs {
            let permit = sem
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| anyhow::anyhow!("validation semaphore closed: {e}"))?;
            let repo = Arc::clone(&self.repo);
            let cbpro_client = self.cbpro_client.clone();
            let timeout_secs = self.config.validation.default_timeout_secs;
            set.spawn(async move {
                let _permit = permit;
                process_validation_job(repo, cbpro_client, job, timeout_secs).await
            });
        }
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    warn!(error = %error, "Validation job processing failed");
                }
                Err(error) => {
                    warn!(error = %error, "Validation job task panicked or aborted");
                }
            }
        }
        Ok(())
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn refresh_runtime_decisions_from_greptime(&self) -> Result<()> {
        let Some(client) = self.greptime_client.as_ref() else {
            return Ok(());
        };
        if let Some(snapshot) = client.query_latency_snapshot().await? {
            let mut decisions = Vec::new();
            for (_key, cell) in &self.runtime_state {
                let mut state = cell.lock();
                if let Some(p50) = snapshot.p50_ms {
                    state.decision.latency_baseline_ms =
                        mazbri_core::num::f64_to_u64(p50.round().max(1.0));
                    state.decision.latency_p50_ms = state.decision.latency_baseline_ms;
                }
                if let Some(p75) = snapshot.p75_ms {
                    state.decision.latency_p75_ms =
                        mazbri_core::num::f64_to_u64(p75.round().max(1.0));
                }
                if let Some(p95) = snapshot.p95_ms {
                    state.decision.latency_p95_ms =
                        mazbri_core::num::f64_to_u64(p95.round().max(1.0));
                }
                decisions.push(state.decision.clone());
            }
            for decision in decisions {
                if let Err(error) = self.repo.upsert_runtime_decision(&decision).await {
                    warn!(error = %error, "Failed to persist greptime refreshed runtime decision");
                }
            }
        }
        Ok(())
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn refresh_runtime_decisions_from_db(&self) -> Result<()> {
        let rows = self.repo.restore_runtime_decisions().await?;
        for decision in rows {
            let key = if decision.cache_key.len() == 64
                && decision.cache_key.bytes().all(|b| b.is_ascii_hexdigit())
            {
                decision.cache_key.to_ascii_lowercase()
            } else {
                canonical_runtime_decision_key(
                    &decision.tenant_id,
                    &decision.product_id,
                    &decision.method,
                    &decision.path,
                    &decision.cache_key,
                )
            };
            let mut normalized = decision;
            normalized.cache_key.clone_from(&key);
            self.runtime_state.insert(
                key,
                Arc::new(Mutex::new(RuntimeDecisionState {
                    baseline_ema_ms: mazbri_core::num::u64_to_f64(normalized.latency_baseline_ms),
                    hit_count_since_validate: 0,
                    last_hit_at_unix_ms: 0,
                    last_validated_at_unix_ms: 0,
                    decision: normalized,
                })),
            );
        }
        Ok(())
    }

    pub fn restore_runtime_decisions_from_platform(&self) {
        if self.repo.platform_db().is_some() {
            info!("platform database configured for cbpro-ext");
        }
    }
}
