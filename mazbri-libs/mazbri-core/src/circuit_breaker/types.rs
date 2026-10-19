#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerEvent {
    StateTransition,
    RequestRejected,
    RequestSuccess,
    RequestFailure,
    CooldownElapsed,
}
impl std::fmt::Display for CircuitBreakerEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitBreakerEvent::StateTransition => write!(f, "state_transition"),
            CircuitBreakerEvent::RequestRejected => write!(f, "request_rejected"),
            CircuitBreakerEvent::RequestSuccess => write!(f, "request_success"),
            CircuitBreakerEvent::RequestFailure => write!(f, "request_failure"),
            CircuitBreakerEvent::CooldownElapsed => write!(f, "cooldown_elapsed"),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitResult<T> {
    Success(T),
    Rejected {
        reason: String,
        retry_after: std::time::Duration,
    },
    Failure {
        error: String,
    },
}
impl<T> CircuitResult<T> {
    pub fn is_success(&self) -> bool {
        matches!(self, CircuitResult::Success(_))
    }
    pub fn is_rejected(&self) -> bool {
        matches!(self, CircuitResult::Rejected { .. })
    }
    pub fn is_failure(&self) -> bool {
        matches!(self, CircuitResult::Failure { .. })
    }
    pub fn ok(&self) -> Option<&T> {
        match self {
            CircuitResult::Success(v) => Some(v),
            _ => None,
        }
    }
    pub fn into_result(self) -> Result<T, String> {
        match self {
            CircuitResult::Success(v) => Ok(v),
            CircuitResult::Rejected { reason, .. } => Err(reason),
            CircuitResult::Failure { error } => Err(error),
        }
    }
}
