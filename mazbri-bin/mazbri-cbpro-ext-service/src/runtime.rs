use crate::config::RuntimeConfig;
use mazbri_rpc::{RuntimeDecision, RuntimeDecisionRequest};
use sha2::{Digest, Sha256};

const MAX_DB_PATH_CHARS: usize = 512;

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        return s.to_string();
    }
    s.chars().take(max_chars).collect()
}

/// Computes a deterministic fixed 64-character lowercase SHA-256 hex key for `runtime_decisions`
/// so arbitrary query/body/path lengths never exceed PostgreSQL's `VARCHAR(500)` / B-tree limit
/// and every read/write path resolves the exact same primary key.
#[must_use]
pub fn canonical_runtime_decision_key(
    tenant_id: &str,
    product_id: &str,
    method: &str,
    path: &str,
    raw_cache_key: &str,
) -> String {
    let method_upper = method.trim().to_ascii_uppercase();
    let mut hasher = Sha256::new();
    for segment in [tenant_id, product_id, method_upper.as_str(), path, raw_cache_key] {
        hasher.update((segment.len() as u64).to_be_bytes());
        hasher.update(segment.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Ensures `validation_jobs.cache_key` is bounded to at most 64 hex chars if a caller sends
/// a long raw key while preserving 64-char hex SHA-256 fallback keys sent by `mazbri-gateway`.
#[must_use]
pub fn bounded_validation_cache_key(raw_cache_key: &str) -> String {
    let trimmed = raw_cache_key.trim();
    if trimmed.len() == 64 && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        trimmed.to_ascii_lowercase()
    } else if trimmed.len() <= 256 {
        trimmed.to_string()
    } else {
        hex::encode(Sha256::digest(trimmed.as_bytes()))
    }
}

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
        decision.valid_until_unix_ms =
            now_unix_ms + mazbri_core::num::millis_i64(config.decision_ttl());
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
                let baseline = mazbri_core::num::f64_to_u64(
                    self.baseline_ema_ms
                        .round()
                        .clamp(1.0, mazbri_core::num::u64_to_f64(u64::MAX)),
                );
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
        self.decision.valid_until_unix_ms =
            now_unix_ms + mazbri_core::num::millis_i64(config.decision_ttl());
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
    canonical_runtime_decision_key(
        &request.tenant_id,
        &request.product_id,
        &request.method,
        &request.path,
        &request.cache_key,
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
    let canonical_key = build_runtime_decision_cache_key(request);
    RuntimeDecision {
        cache_key: canonical_key,
        tenant_id: request.tenant_id.clone(),
        product_id: request.product_id.clone(),
        method: request.method.trim().to_ascii_uppercase(),
        path: truncate_chars(&request.path, MAX_DB_PATH_CHARS),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_decision_key_is_always_64_hex_chars_even_for_huge_inputs() {
        let huge_path = format!("/v1/{}", "a".repeat(8_192));
        let huge_raw = "q=".to_string() + &"b".repeat(16_384);
        let k = canonical_runtime_decision_key("t-1", "8000000001", "post", &huge_path, &huge_raw);
        assert_eq!(k.len(), 64);
        assert!(k.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(
            k,
            canonical_runtime_decision_key("t-1", "8000000001", "POST", &huge_path, &huge_raw)
        );
        assert_ne!(
            k,
            canonical_runtime_decision_key("t-2", "8000000001", "POST", &huge_path, &huge_raw)
        );
    }
}
