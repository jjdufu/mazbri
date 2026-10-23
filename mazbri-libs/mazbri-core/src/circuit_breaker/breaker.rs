use super::config::CircuitBreakerConfig;
use super::metrics::{record_failure, record_state, record_success_rate};
use super::state::{CircuitState, circuit_values as state_constants};
use super::types::{CircuitBreakerEvent, CircuitResult};
use crate::num::SaturatingFrom;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU8, AtomicU64, Ordering};
use std::time::Duration;
use tracing::{debug, info, warn};
pub(crate) fn log_event(
    name: &str,
    event: CircuitBreakerEvent,
    from_state: Option<CircuitState>,
    to_state: Option<CircuitState>,
    details: &str,
) {
    match event {
        CircuitBreakerEvent::StateTransition => {
            info!(
                circuit_breaker = % name, event = % event, from_state = ? from_state,
                to_state = ? to_state, details = % details,
                "Circuit breaker state transition"
            );
        }
        CircuitBreakerEvent::RequestRejected => {
            warn!(
                circuit_breaker = % name, event = % event, state = ? from_state, details
                = % details, "Circuit breaker request rejected"
            );
        }
        CircuitBreakerEvent::RequestSuccess => {
            debug!(
                circuit_breaker = % name, event = % event, state = ? from_state, details
                = % details, "Circuit breaker request succeeded"
            );
        }
        CircuitBreakerEvent::RequestFailure => {
            warn!(
                circuit_breaker = % name, event = % event, state = ? from_state, details
                = % details, "Circuit breaker request failed"
            );
        }
        CircuitBreakerEvent::CooldownElapsed => {
            info!(
                circuit_breaker = % name, event = % event, from_state = ? from_state,
                to_state = ? to_state, "Circuit breaker cooldown elapsed"
            );
        }
    }
}
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<AtomicU8>,
    failure_count: Arc<AtomicU64>,
    success_count: Arc<AtomicU64>,
    last_state_change_ms: Arc<AtomicI64>,
    name: String,
}
impl CircuitBreaker {
    pub fn new(name: String, config: CircuitBreakerConfig) -> Self {
        let state = Arc::new(AtomicU8::new(state_constants::CLOSED));
        let last_state_change_ms = Arc::new(AtomicI64::new(Self::now_ms()));
        record_state(&name, CircuitState::Closed);
        debug!(
            circuit_breaker = % name, failure_threshold = config.failure_threshold,
            success_threshold = config.success_threshold, cooldown_secs = config
            .cooldown_duration.as_secs(), "Circuit breaker created"
        );
        Self {
            config,
            state,
            failure_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            last_state_change_ms,
            name,
        }
    }
    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::saturating_from(d.as_millis()))
    }
    #[must_use]
    pub fn state(&self) -> CircuitState {
        CircuitState::from_u8(self.state.load(Ordering::Acquire))
    }
    #[must_use]
    pub fn failure_count(&self) -> u64 {
        self.failure_count.load(Ordering::Relaxed)
    }
    #[must_use]
    pub fn success_count(&self) -> u64 {
        self.success_count.load(Ordering::Relaxed)
    }
    #[must_use]
    pub fn time_since_last_state_change(&self) -> Duration {
        let last_change_ms = self.last_state_change_ms.load(Ordering::Acquire);
        let now_ms = Self::now_ms();
        Duration::from_millis(u64::saturating_from((now_ms - last_change_ms).max(0)))
    }
    #[must_use]
    pub fn is_request_allowed(&self) -> bool {
        let current_state = CircuitState::from_u8(self.state.load(Ordering::Acquire));
        match current_state {
            CircuitState::Open => {
                let elapsed = self.time_since_last_state_change();
                elapsed >= self.config.cooldown_duration
            }
            CircuitState::Closed | CircuitState::HalfOpen => true,
        }
    }
    pub async fn execute<F, T, Fut>(&self, request_fn: F) -> CircuitResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        let current_state = CircuitState::from_u8(self.state.load(Ordering::Acquire));
        match current_state {
            CircuitState::Open => self.handle_open_state(),
            CircuitState::HalfOpen => {
                let result = request_fn().await;
                self.handle_half_open(result)
            }
            CircuitState::Closed => {
                let result = request_fn().await;
                self.handle_closed(result)
            }
        }
    }
    fn handle_open_state<T>(&self) -> CircuitResult<T> {
        let elapsed = self.time_since_last_state_change();
        let cooldown_passed = elapsed >= self.config.cooldown_duration;
        if cooldown_passed {
            log_event(
                &self.name,
                CircuitBreakerEvent::CooldownElapsed,
                Some(CircuitState::Open),
                Some(CircuitState::HalfOpen),
                "Cooldown period elapsed, transitioning to half-open",
            );
            self.transition_to(CircuitState::HalfOpen);
            return CircuitResult::Rejected {
                reason: "断路器转换到半开".to_string(),
                retry_after: Duration::from_secs(0),
            };
        }
        let retry_after = self.config.cooldown_duration.saturating_sub(elapsed);
        log_event(
            &self.name,
            CircuitBreakerEvent::RequestRejected,
            Some(CircuitState::Open),
            None,
            &format!("Circuit is open, retry after {retry_after:?}"),
        );
        CircuitResult::Rejected {
            reason: "断路器打开".to_string(),
            retry_after,
        }
    }
    fn handle_half_open<T>(&self, result: Result<T, String>) -> CircuitResult<T> {
        match result {
            Ok(value) => {
                let successes = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
                let total_requests = successes + self.failure_count.load(Ordering::Relaxed);
                record_success_rate(&self.name, successes, total_requests);
                log_event(
                    &self.name,
                    CircuitBreakerEvent::RequestSuccess,
                    Some(CircuitState::HalfOpen),
                    None,
                    &format!(
                        "Test request succeeded ({}/{})",
                        successes, self.config.success_threshold
                    ),
                );
                if successes >= self.config.success_threshold {
                    log_event(
                        &self.name,
                        CircuitBreakerEvent::StateTransition,
                        Some(CircuitState::HalfOpen),
                        Some(CircuitState::Closed),
                        &format!("Success threshold ({successes}) reached"),
                    );
                    self.transition_to(CircuitState::Closed);
                    self.reset_counters();
                }
                CircuitResult::Success(value)
            }
            Err(error) => {
                record_failure(&self.name, &error);
                log_event(
                    &self.name,
                    CircuitBreakerEvent::RequestFailure,
                    Some(CircuitState::HalfOpen),
                    None,
                    &format!("Test request failed: {error}"),
                );
                log_event(
                    &self.name,
                    CircuitBreakerEvent::StateTransition,
                    Some(CircuitState::HalfOpen),
                    Some(CircuitState::Open),
                    "Test request failed, reopening circuit",
                );
                self.transition_to(CircuitState::Open);
                self.reset_counters();
                CircuitResult::Failure { error }
            }
        }
    }
    fn handle_closed<T>(&self, result: Result<T, String>) -> CircuitResult<T> {
        match result {
            Ok(value) => {
                self.reset_failure_count_if_nonzero();
                log_event(
                    &self.name,
                    CircuitBreakerEvent::RequestSuccess,
                    Some(CircuitState::Closed),
                    None,
                    "Request succeeded",
                );
                CircuitResult::Success(value)
            }
            Err(error) => {
                let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
                record_failure(&self.name, &error);
                log_event(
                    &self.name,
                    CircuitBreakerEvent::RequestFailure,
                    Some(CircuitState::Closed),
                    None,
                    &format!(
                        "Request failed ({}/{})",
                        failures, self.config.failure_threshold
                    ),
                );
                if failures >= self.config.failure_threshold {
                    log_event(
                        &self.name,
                        CircuitBreakerEvent::StateTransition,
                        Some(CircuitState::Closed),
                        Some(CircuitState::Open),
                        &format!("Failure threshold ({failures}) reached"),
                    );
                    self.transition_to(CircuitState::Open);
                    self.reset_counters();
                }
                CircuitResult::Failure { error }
            }
        }
    }
    pub fn transition_to(&self, new_state: CircuitState) {
        let old_state = CircuitState::from_u8(self.state.swap(new_state.to_u8(), Ordering::AcqRel));
        self.last_state_change_ms
            .store(Self::now_ms(), Ordering::Release);
        record_state(&self.name, new_state);
        info!(
            circuit_breaker = % self.name, from = % old_state, to = % new_state,
            "Circuit breaker state transition"
        );
    }
    fn reset_failure_count_if_nonzero(&self) {
        self.failure_count.swap(0, Ordering::AcqRel);
    }
    fn reset_counters(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
    }
    pub fn reset(&self) {
        self.reset_counters();
        self.transition_to(CircuitState::Closed);
    }
}
