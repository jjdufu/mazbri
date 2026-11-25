use super::config::CircuitBreakerConfig;
use super::metrics::{record_failure, record_state, record_success_rate};
use super::state::{CircuitState, circuit_values as state_constants};
use super::types::{CircuitBreakerEvent, CircuitResult};
use crate::num::SaturatingFrom;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};
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
                circuit_breaker = %name,
                event = %event,
                from_state = ?from_state,
                to_state = ?to_state,
                details = %details,
                "Circuit breaker state transition"
            );
        }
        CircuitBreakerEvent::RequestRejected => {
            warn!(
                circuit_breaker = %name,
                event = %event,
                state = ?from_state,
                details = %details,
                "Circuit breaker request rejected"
            );
        }
        CircuitBreakerEvent::RequestSuccess => {
            debug!(
                circuit_breaker = %name,
                event = %event,
                state = ?from_state,
                details = %details,
                "Circuit breaker request succeeded"
            );
        }
        CircuitBreakerEvent::RequestFailure => {
            warn!(
                circuit_breaker = %name,
                event = %event,
                state = ?from_state,
                details = %details,
                "Circuit breaker request failed"
            );
        }
        CircuitBreakerEvent::CooldownElapsed => {
            info!(
                circuit_breaker = %name,
                event = %event,
                from_state = ?from_state,
                to_state = ?to_state,
                "Circuit breaker cooldown elapsed"
            );
        }
    }
}

struct HalfOpenInFlightGuard<'a>(&'a AtomicU32);

impl Drop for HalfOpenInFlightGuard<'_> {
    fn drop(&mut self) {
        let _ = self
            .0
            .try_update(Ordering::AcqRel, Ordering::Acquire, |cur| {
                Some(cur.saturating_sub(1))
            });
    }
}

pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<AtomicU8>,
    failure_count: Arc<AtomicU64>,
    success_count: Arc<AtomicU64>,
    half_open_in_flight: Arc<AtomicU32>,
    origin: Instant,
    last_state_change_ns: Arc<AtomicU64>,
    name: String,
}

impl CircuitBreaker {
    pub fn new(name: String, config: CircuitBreakerConfig) -> Self {
        let state = Arc::new(AtomicU8::new(state_constants::CLOSED));
        let origin = Instant::now();
        let last_state_change_ns = Arc::new(AtomicU64::new(0));
        record_state(&name, CircuitState::Closed);
        debug!(
            circuit_breaker = %name,
            failure_threshold = config.failure_threshold,
            success_threshold = config.success_threshold,
            cooldown_secs = config.cooldown_duration.as_secs(),
            half_open_max_calls = config.half_open_max_calls,
            "Circuit breaker created"
        );
        Self {
            config,
            state,
            failure_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            half_open_in_flight: Arc::new(AtomicU32::new(0)),
            origin,
            last_state_change_ns,
            name,
        }
    }

    fn now_offset_ns(&self) -> u64 {
        u64::saturating_from(self.origin.elapsed().as_nanos())
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
    pub fn half_open_in_flight(&self) -> u32 {
        self.half_open_in_flight.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn time_since_last_state_change(&self) -> Duration {
        let last_ns = self.last_state_change_ns.load(Ordering::Acquire);
        let now_ns = self.now_offset_ns();
        Duration::from_nanos(now_ns.saturating_sub(last_ns))
    }

    #[must_use]
    pub fn is_request_allowed(&self) -> bool {
        let current_state = CircuitState::from_u8(self.state.load(Ordering::Acquire));
        match current_state {
            CircuitState::Open => {
                self.time_since_last_state_change() >= self.config.cooldown_duration
            }
            CircuitState::HalfOpen => {
                self.half_open_in_flight.load(Ordering::Acquire)
                    < self.config.half_open_max_calls.max(1)
            }
            CircuitState::Closed => true,
        }
    }

    pub async fn execute<F, T, Fut>(&self, request_fn: F) -> CircuitResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        self.execute_with_classifier(request_fn, |_| None).await
    }

    pub async fn execute_with_classifier<F, T, Fut, C>(
        &self,
        request_fn: F,
        classify_err: C,
    ) -> CircuitResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
        C: FnOnce(&T) -> Option<String>,
    {
        let mut current_state = CircuitState::from_u8(self.state.load(Ordering::Acquire));
        if current_state == CircuitState::Open {
            let elapsed = self.time_since_last_state_change();
            if elapsed >= self.config.cooldown_duration {
                match self.state.compare_exchange(
                    state_constants::OPEN,
                    state_constants::HALF_OPEN,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        self.last_state_change_ns
                            .store(self.now_offset_ns(), Ordering::Release);
                        self.reset_counters();
                        record_state(&self.name, CircuitState::HalfOpen);
                        log_event(
                            &self.name,
                            CircuitBreakerEvent::CooldownElapsed,
                            Some(CircuitState::Open),
                            Some(CircuitState::HalfOpen),
                            "Cooldown period elapsed, transitioning to half-open",
                        );
                        current_state = CircuitState::HalfOpen;
                    }
                    Err(actual) => {
                        current_state = CircuitState::from_u8(actual);
                    }
                }
            }
        }

        match current_state {
            CircuitState::Open => {
                let elapsed = self.time_since_last_state_change();
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
            CircuitState::HalfOpen => {
                let max_calls = self.config.half_open_max_calls.max(1);
                let admitted = self.half_open_in_flight.try_update(
                    Ordering::AcqRel,
                    Ordering::Acquire,
                    |cur| (cur < max_calls).then_some(cur + 1),
                );
                if admitted.is_err() {
                    log_event(
                        &self.name,
                        CircuitBreakerEvent::RequestRejected,
                        Some(CircuitState::HalfOpen),
                        None,
                        "Half-open probe concurrency limit reached",
                    );
                    return CircuitResult::Rejected {
                        reason: "断路器半开探测并发超限".to_string(),
                        retry_after: Duration::from_millis(50),
                    };
                }
                let guard = HalfOpenInFlightGuard(&self.half_open_in_flight);
                let result = request_fn().await;
                drop(guard);
                self.handle_half_open(result, classify_err)
            }
            CircuitState::Closed => {
                let result = request_fn().await;
                self.handle_closed(result, classify_err)
            }
        }
    }

    /// Explicitly records an external failure (such as an outer timeout cancellation).
    pub fn record_failure(&self, error: &str) {
        match self.state() {
            CircuitState::HalfOpen => self.record_half_open_failure(error),
            CircuitState::Closed => self.record_closed_failure(error),
            CircuitState::Open => {
                record_failure(&self.name, error);
            }
        }
    }

    /// Explicitly records an external success.
    pub fn record_success(&self) {
        match self.state() {
            CircuitState::HalfOpen => self.record_half_open_success(),
            CircuitState::Closed => self.record_closed_success(),
            CircuitState::Open => {}
        }
    }

    fn record_half_open_success(&self) {
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
        if successes >= self.config.success_threshold
            && self
                .state
                .compare_exchange(
                    state_constants::HALF_OPEN,
                    state_constants::CLOSED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
        {
            self.last_state_change_ns
                .store(self.now_offset_ns(), Ordering::Release);
            self.reset_counters();
            record_state(&self.name, CircuitState::Closed);
            log_event(
                &self.name,
                CircuitBreakerEvent::StateTransition,
                Some(CircuitState::HalfOpen),
                Some(CircuitState::Closed),
                &format!("Success threshold ({successes}) reached"),
            );
        }
    }

    fn record_half_open_failure(&self, error: &str) {
        record_failure(&self.name, error);
        log_event(
            &self.name,
            CircuitBreakerEvent::RequestFailure,
            Some(CircuitState::HalfOpen),
            None,
            &format!("Test request failed: {error}"),
        );
        if self
            .state
            .compare_exchange(
                state_constants::HALF_OPEN,
                state_constants::OPEN,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            self.last_state_change_ns
                .store(self.now_offset_ns(), Ordering::Release);
            self.reset_counters();
            record_state(&self.name, CircuitState::Open);
            log_event(
                &self.name,
                CircuitBreakerEvent::StateTransition,
                Some(CircuitState::HalfOpen),
                Some(CircuitState::Open),
                "Test request failed, reopening circuit",
            );
        }
    }

    fn record_closed_success(&self) {
        self.reset_failure_count_if_nonzero();
        log_event(
            &self.name,
            CircuitBreakerEvent::RequestSuccess,
            Some(CircuitState::Closed),
            None,
            "Request succeeded",
        );
    }

    fn record_closed_failure(&self, error: &str) {
        let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        record_failure(&self.name, error);
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
        if failures >= self.config.failure_threshold
            && self
                .state
                .compare_exchange(
                    state_constants::CLOSED,
                    state_constants::OPEN,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
        {
            self.last_state_change_ns
                .store(self.now_offset_ns(), Ordering::Release);
            self.reset_counters();
            record_state(&self.name, CircuitState::Open);
            log_event(
                &self.name,
                CircuitBreakerEvent::StateTransition,
                Some(CircuitState::Closed),
                Some(CircuitState::Open),
                &format!("Failure threshold ({failures}) reached"),
            );
        }
    }

    fn handle_half_open<T, C>(&self, result: Result<T, String>, classify_err: C) -> CircuitResult<T>
    where
        C: FnOnce(&T) -> Option<String>,
    {
        match result {
            Ok(value) => {
                if let Some(error) = classify_err(&value) {
                    self.record_half_open_failure(&error);
                } else {
                    self.record_half_open_success();
                }
                CircuitResult::Success(value)
            }
            Err(error) => {
                self.record_half_open_failure(&error);
                CircuitResult::Failure { error }
            }
        }
    }

    fn handle_closed<T, C>(&self, result: Result<T, String>, classify_err: C) -> CircuitResult<T>
    where
        C: FnOnce(&T) -> Option<String>,
    {
        match result {
            Ok(value) => {
                if let Some(error) = classify_err(&value) {
                    self.record_closed_failure(&error);
                } else {
                    self.record_closed_success();
                }
                CircuitResult::Success(value)
            }
            Err(error) => {
                self.record_closed_failure(&error);
                CircuitResult::Failure { error }
            }
        }
    }

    pub fn transition_to(&self, new_state: CircuitState) {
        let old_state = CircuitState::from_u8(self.state.swap(new_state.to_u8(), Ordering::AcqRel));
        self.last_state_change_ns
            .store(self.now_offset_ns(), Ordering::Release);
        record_state(&self.name, new_state);
        info!(
            circuit_breaker = %self.name,
            from = %old_state,
            to = %new_state,
            "Circuit breaker state transition"
        );
    }

    fn reset_failure_count_if_nonzero(&self) {
        if self.failure_count.load(Ordering::Relaxed) > 0 {
            self.failure_count.store(0, Ordering::Release);
        }
    }

    fn reset_counters(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        self.half_open_in_flight.store(0, Ordering::Release);
    }

    pub fn reset(&self) {
        self.reset_counters();
        self.transition_to(CircuitState::Closed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_cooldown_cas_admits_probe_and_caps_half_open_concurrency() {
        let cb = Arc::new(CircuitBreaker::new(
            "channel:ch-1".to_string(),
            CircuitBreakerConfig {
                failure_threshold: 2,
                success_threshold: 2,
                cooldown_duration: Duration::from_millis(20),
                request_timeout: Duration::from_secs(1),
                half_open_max_calls: 2,
            },
        ));
        cb.record_failure("upstream timeout");
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure("upstream timeout");
        assert_eq!(cb.state(), CircuitState::Open);

        tokio::time::sleep(Duration::from_millis(30)).await;

        let barrier = Arc::new(tokio::sync::Notify::new());
        let b1 = Arc::clone(&barrier);
        let cb1 = Arc::clone(&cb);
        let h1 = tokio::spawn(async move {
            cb1.execute(|| async move {
                b1.notified().await;
                Ok::<_, String>(1_u32)
            })
            .await
        });
        let b2 = Arc::clone(&barrier);
        let cb2 = Arc::clone(&cb);
        let h2 = tokio::spawn(async move {
            cb2.execute(|| async move {
                b2.notified().await;
                Ok::<_, String>(2_u32)
            })
            .await
        });

        while cb.half_open_in_flight() < 2 {
            tokio::task::yield_now().await;
        }
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        // Third concurrent request in HalfOpen must be rejected immediately.
        let third = cb.execute(|| async { Ok::<_, String>(3_u32) }).await;
        assert!(matches!(third, CircuitResult::Rejected { .. }));

        barrier.notify_waiters();
        assert!(matches!(h1.await.expect("h1"), CircuitResult::Success(1)));
        assert!(matches!(h2.await.expect("h2"), CircuitResult::Success(2)));
        assert_eq!(cb.state(), CircuitState::Closed);
        assert_eq!(cb.half_open_in_flight(), 0);
    }

    #[tokio::test]
    async fn classified_5xx_response_trips_breaker_and_cancelled_probe_releases_slot() {
        let cb = CircuitBreaker::new(
            "channel:ch-503".to_string(),
            CircuitBreakerConfig {
                failure_threshold: 1,
                success_threshold: 1,
                cooldown_duration: Duration::from_millis(10),
                request_timeout: Duration::from_secs(1),
                half_open_max_calls: 1,
            },
        );
        let out = cb
            .execute_with_classifier(
                || async { Ok::<u16, String>(503) },
                |status| (*status >= 500).then(|| format!("HTTP status {status}")),
            )
            .await;
        assert!(matches!(out, CircuitResult::Success(503)));
        assert_eq!(cb.state(), CircuitState::Open);

        tokio::time::sleep(Duration::from_millis(20)).await;
        // Simulate outer timeout cancelling HalfOpen probe mid-flight.
        let timed_out = tokio::time::timeout(
            Duration::from_millis(10),
            cb.execute(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok::<u16, String>(200)
            }),
        )
        .await;
        assert!(timed_out.is_err());
        assert_eq!(
            cb.half_open_in_flight(),
            0,
            "dropped HalfOpen future must release in-flight slot"
        );
        cb.record_failure("upstream timeout");
        assert_eq!(cb.state(), CircuitState::Open);
    }
}
