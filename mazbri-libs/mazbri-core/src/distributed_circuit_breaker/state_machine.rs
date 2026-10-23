use crate::CircuitState;
use std::time::{Duration, Instant};
pub fn open_state_cooldown_passed(last_state_change: Instant, cooldown_duration: Duration) -> bool {
    last_state_change.elapsed() >= cooldown_duration
}
pub fn state_from_db_value(state_value: i16) -> CircuitState {
    match state_value {
        1 => CircuitState::HalfOpen,
        2 => CircuitState::Open,
        _ => CircuitState::Closed,
    }
}
