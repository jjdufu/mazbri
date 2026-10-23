use std::time::Duration;
#[derive(Clone, Debug)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u64,
    pub success_threshold: u64,
    pub cooldown_duration: Duration,
    pub request_timeout: Duration,
}
impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            cooldown_duration: Duration::from_secs(60),
            request_timeout: Duration::from_secs(30),
        }
    }
}
impl CircuitBreakerConfig {
    #[must_use]
    pub fn aggressive() -> Self {
        Self {
            failure_threshold: 3,
            success_threshold: 2,
            cooldown_duration: Duration::from_secs(30),
            request_timeout: Duration::from_secs(10),
        }
    }
    #[must_use]
    pub fn lenient() -> Self {
        Self {
            failure_threshold: 10,
            success_threshold: 3,
            cooldown_duration: Duration::from_secs(120),
            request_timeout: Duration::from_secs(60),
        }
    }
}
