pub(crate) mod circuit_values {
    pub const CLOSED: u8 = 0;
    pub const HALF_OPEN: u8 = 1;
    pub const OPEN: u8 = 2;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}
impl CircuitState {
    #[must_use]
    pub const fn as_metric_value(&self) -> i64 {
        match self {
            CircuitState::Closed => circuit_values::CLOSED as i64,
            CircuitState::HalfOpen => circuit_values::HALF_OPEN as i64,
            CircuitState::Open => circuit_values::OPEN as i64,
        }
    }
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            CircuitState::Closed => "closed",
            CircuitState::HalfOpen => "half_open",
            CircuitState::Open => "open",
        }
    }
    pub(crate) fn from_u8(value: u8) -> Self {
        match value {
            circuit_values::HALF_OPEN => CircuitState::HalfOpen,
            circuit_values::OPEN => CircuitState::Open,
            _ => CircuitState::Closed,
        }
    }
    pub(crate) fn to_u8(self) -> u8 {
        match self {
            CircuitState::Closed => circuit_values::CLOSED,
            CircuitState::HalfOpen => circuit_values::HALF_OPEN,
            CircuitState::Open => circuit_values::OPEN,
        }
    }
}
impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
