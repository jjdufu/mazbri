use crate::cbpro_client::CBProClient;
use crate::config::CBProExtServiceConfig;
use crate::db::CbProExtRepository;
use crate::greptime::GreptimeClient;
use crate::observations::now_unix_ms;
use crate::runtime::{RuntimeDecisionState, build_runtime_decision_cache_key};
use crate::validation::{build_validation_request, process_validation_job};
use anyhow::Result;
use mazbri_core::num::SaturatingFrom;
use mazbri_rpc::{
    ObservationRequest, ObservationResponse, RuntimeDecision, RuntimeDecisionRequest,
    RuntimeDecisionResponse, ValidationEnqueueRequest, ValidationEnqueueResponse,
    ValidationResultRequest, ValidationResultResponse,
};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

pub struct CbProExtManager {
    config: CBProExtServiceConfig,
    repo: Arc<CbProExtRepository>,
    cbpro_client: Option<Arc<CBProClient>>,
    greptime_client: Option<GreptimeClient>,
    runtime_state: Mutex<HashMap<String, RuntimeDecisionState>>,
}

impl CbProExtManager {
    #[must_use]
    pub fn new(
        config: CBProExtServiceConfig,
        repo: Arc<CbProExtRepository>,
        cbpro_client: Option<Arc<CBProClient>>,
        greptime_client: Option<GreptimeClient>,
    ) -> Self {
        Self {
            config,
            repo,
            cbpro_client,
            greptime_client,
            runtime_state: Mutex::new(HashMap::new()),
        }
    }

    pub async fn record_observation(&self, request: ObservationRequest) -> ObservationResponse {
        let Some(cache_key) = request.cache_key.clone() else {
            return ObservationResponse {
                success: true,
                error: None,
            };
        };
        let runtime_key = format!(
            "{}:{}:{}:{}:{}",
            request.tenant_id, request.product_id, request.method, request.path, cache_key
        );
        let decision = {
            let mut guard = self.runtime_state.lock();
            let state = guard.entry(runtime_key.clone()).or_insert_with(|| {
                RuntimeDecisionState::new(
                    &RuntimeDecisionRequest {
                        tenant_id: request.tenant_id.clone(),
                        product_id: request.product_id.clone(),
                        method: request.method.clone(),
                        path: request.path.clone(),
                        cache_key: cache_key.clone(),
                        policy_version: None,
                    },
                    &self.config.runtime,
                    now_unix_ms(),
                )
            });
            let validate_due = state.observe(
                &request.event_type,
                request.service_total_ms,
                &self.config.runtime,
                now_unix_ms(),
            );
            (state.decision.clone(), validate_due)
        };
        let (decision, validate_due) = decision;
        if let Err(error) = self.repo.upsert_runtime_decision(&decision).await {
            warn!(error = % error, cache_key = % cache_key, "Failed to persist runtime decision");
        }
        if validate_due && self.config.validation.enabled {
            let enqueue_request = build_validation_request(
                cache_key.clone(),
                request.tenant_id.clone(),
                request.product_id.clone(),
                request.method.clone(),
                request.path.clone(),
                format!("event:{}", request.event_type),
            );
            let response = self.enqueue_validation(enqueue_request).await;
            if !response.accepted {
                if let Some(error) = response.error {
                    warn!(error = % error.message(), "Failed to enqueue validation from observation");
                }
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
        if let Some(decision) = {
            let guard = self.runtime_state.lock();
            guard.get(&key).map(|state| state.decision.clone())
        } {
            return RuntimeDecisionResponse {
                available: true,
                decision: Some(decision),
                error: None,
            };
        }
        match self.repo.get_runtime_decision(&key).await {
            Ok(Some(decision)) => {
                let mut guard = self.runtime_state.lock();
                guard.insert(
                    key.clone(),
                    RuntimeDecisionState {
                        baseline_ema_ms: mazbri_core::num::u64_to_f64(decision.latency_baseline_ms),
                        hit_count_since_validate: 0,
                        last_hit_at_unix_ms: 0,
                        last_validated_at_unix_ms: 0,
                        decision: decision.clone(),
                    },
                );
                RuntimeDecisionResponse {
                    available: true,
                    decision: Some(decision),
                    error: None,
                }
            }
            Ok(None) => {
                let decision = {
                    let mut guard = self.runtime_state.lock();
                    if guard.len() >= self.config.runtime.max_runtime_entries
                        && let Some(first_key) = guard.keys().next().cloned()
                    {
                        guard.remove(&first_key);
                    }
                    let state =
                        RuntimeDecisionState::new(&request, &self.config.runtime, now_unix_ms());
                    let decision = state.decision.clone();
                    guard.insert(key.clone(), state);
                    decision
                };
                if let Err(error) = self.repo.upsert_runtime_decision(&decision).await {
                    warn!(error = % error, cache_key = % key, "Failed to seed runtime decision");
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
        request: ValidationEnqueueRequest,
    ) -> ValidationEnqueueResponse {
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
        let key = format!(
            "{}:{}:{}:{}:{}",
            request.tenant_id, request.product_id, request.method, request.path, request.cache_key
        );
        let decision_to_persist = {
            let mut guard = self.runtime_state.lock();
            if let Some(state) = guard.get_mut(&key) {
                state.mark_validated(request.message.clone(), now_unix_ms());
                Some(state.decision.clone())
            } else {
                None
            }
        };
        if let Some(decision) = decision_to_persist
            && let Err(error) = self.repo.upsert_runtime_decision(&decision).await
        {
            warn!(error = % error, cache_key = % key, "Failed to persist validation result");
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
            warn!(error = % error, cache_key = % key, "Failed to record validation job result");
        }
        ValidationResultResponse {
            recorded: true,
            error: None,
        }
    }

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
        for job in jobs {
            if let Err(error) = process_validation_job(
                Arc::clone(&self.repo),
                self.cbpro_client.clone(),
                job,
                self.config.validation.default_timeout_secs,
            )
            .await
            {
                warn!(error = % error, "Validation job processing failed");
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
            let decisions: Vec<RuntimeDecision> = {
                let mut guard = self.runtime_state.lock();
                for state in guard.values_mut() {
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
                }
                guard.values().map(|state| state.decision.clone()).collect()
            };
            for decision in decisions {
                if let Err(error) = self.repo.upsert_runtime_decision(&decision).await {
                    warn!(error = % error, "Failed to persist greptime refreshed runtime decision");
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
        let mut guard = self.runtime_state.lock();
        for decision in rows {
            let key = format!(
                "{}:{}:{}:{}:{}",
                decision.tenant_id,
                decision.product_id,
                decision.method,
                decision.path,
                decision.cache_key
            );
            guard.insert(
                key,
                RuntimeDecisionState {
                    baseline_ema_ms: mazbri_core::num::u64_to_f64(decision.latency_baseline_ms),
                    hit_count_since_validate: 0,
                    last_hit_at_unix_ms: 0,
                    last_validated_at_unix_ms: 0,
                    decision,
                },
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
