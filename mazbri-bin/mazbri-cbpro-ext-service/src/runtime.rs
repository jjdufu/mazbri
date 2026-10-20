use crate::config::RuntimeConfig;
use mazbri_rpc::{RuntimeDecision, RuntimeDecisionRequest};

#[derive(Debug, Clone)]
pub struct RuntimeDecisionState {
    pub decision: RuntimeDecision,
    pub hit_count_since_validate: u64,
    pub baseline_ema_ms: f64,
    pub last_hit_at_unix_ms: i64,
    pub last_validated_at_unix_ms: i64,
}

impl RuntimeDecisionState {
    #[must_use]
    pub fn new(request: &RuntimeDecisionRequest, config: &RuntimeConfig, now_unix_ms: i64) -> Self {
        let mut decision = build_default_runtime_decision(request, config);
        let baseline_ema_ms = mazbri_core::num::u64_to_f64(decision.latency_baseline_ms);
        decision.valid_until_unix_ms = now_unix_ms + mazbri_core::num::millis_i64(config.decision_ttl());
        decision.generated_at_unix_ms = now_unix_ms;
        Self {
            decision,
            hit_count_since_validate: 0,
            baseline_ema_ms,
            last_hit_at_unix_ms: 0,
            last_validated_at_unix_ms: 0,
        }
    }

    pub fn observe(
        &mut self,
        event_type: &str,
        service_total_ms: Option<u64>,
        config: &RuntimeConfig,
        now_unix_ms: i64,
    ) -> bool {
        let mut validate_due = false;
        if is_hit_event(event_type) {
            self.decision.hot_level = self.decision.hot_level.saturating_add(1).min(3);
            self.last_hit_at_unix_ms = now_unix_ms;
            self.hit_count_since_validate = self.hit_count_since_validate.saturating_add(1);
            if let Some(service_total_ms) = service_total_ms {
                let service_total_ms = mazbri_core::num::u64_to_f64(service_total_ms.max(1));
                self.baseline_ema_ms = (self.baseline_ema_ms * 0.8) + (service_total_ms * 0.2);
                let baseline = mazbri_core::num::f64_to_u64(self.baseline_ema_ms.round().clamp(1.0, mazbri_core::num::u64_to_f64(u64::MAX)));
                self.decision.latency_baseline_ms = baseline;
                self.decision.latency_p50_ms = baseline;
                self.decision.latency_p75_ms = baseline.saturating_add(baseline / 10);
                self.decision.latency_p95_ms = baseline.saturating_add(baseline / 4);
            }
            if self.hit_count_since_validate >= 10 {
                validate_due = true;
            }
        }
        if let Some(service_total_ms) = service_total_ms {
            self.decision.current_ttl_secs = self
                .decision
                .current_ttl_secs
                .max((service_total_ms / 1_000).max(1));
        }
        self.decision.validate_due = validate_due;
        self.decision.valid_until_unix_ms = now_unix_ms + mazbri_core::num::millis_i64(config.decision_ttl());
        self.decision.generated_at_unix_ms = now_unix_ms;
        validate_due
    }

    pub fn mark_validated(&mut self, status: Option<String>, now_unix_ms: i64) {
        self.decision.last_validation_status = status;
        self.decision.validate_due = false;
        self.hit_count_since_validate = 0;
        self.last_validated_at_unix_ms = now_unix_ms;
        self.decision.valid_until_unix_ms = now_unix_ms + 5_000;
        self.decision.generated_at_unix_ms = now_unix_ms;
    }
}

#[must_use]
pub fn now_unix_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[must_use]
pub fn build_runtime_decision_cache_key(request: &RuntimeDecisionRequest) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        request.tenant_id, request.product_id, request.method, request.path, request.cache_key
    )
}

#[must_use]
pub fn is_hit_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "cbpro_cache_hit" | "upstream_success" | "cbpro_fallback_hit"
    )
}

#[must_use]
pub fn build_default_runtime_decision(
    request: &RuntimeDecisionRequest,
    config: &RuntimeConfig,
) -> RuntimeDecision {
    let baseline = config.default_latency_baseline_ms.max(1);
    let now = now_unix_ms();
    RuntimeDecision {
        cache_key: request.cache_key.clone(),
        tenant_id: request.tenant_id.clone(),
        product_id: request.product_id.clone(),
        method: request.method.clone(),
        path: request.path.clone(),
        mode: "fallback_only".to_string(),
        policy_version: request.policy_version.unwrap_or(0),
        latency_baseline_ms: baseline,
        latency_p50_ms: baseline,
        latency_p75_ms: baseline.saturating_add(baseline / 10),
        latency_p95_ms: baseline.saturating_add(baseline / 4),
        jitter_min_ms: 0,
        jitter_max_ms: 20,
        max_simulated_delay_ms: 1_000,
        current_ttl_secs: 300,
        validate_due: false,
        hot_level: 0,
        last_validation_status: None,
        valid_until_unix_ms: now + mazbri_core::num::millis_i64(config.decision_ttl()),
        generated_at_unix_ms: now,
    }
}
