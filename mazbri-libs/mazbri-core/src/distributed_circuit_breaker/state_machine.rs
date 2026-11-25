use crate::CircuitState;
use crate::num::millis_i64;
use chrono::Utc;
use std::time::Duration;

#[must_use]
pub fn open_state_cooldown_passed_at(
    now_unix_ms: i64,
    last_state_change_unix_ms: i64,
    cooldown_duration: Duration,
) -> bool {
    let cooldown_ms = millis_i64(cooldown_duration);
    now_unix_ms.saturating_sub(last_state_change_unix_ms) >= cooldown_ms
}

#[must_use]
pub fn open_state_cooldown_passed(
    last_state_change_unix_ms: i64,
    cooldown_duration: Duration,
) -> bool {
    open_state_cooldown_passed_at(
        Utc::now().timestamp_millis(),
        last_state_change_unix_ms,
        cooldown_duration,
    )
}

#[must_use]
pub fn remaining_cooldown_at(
    now_unix_ms: i64,
    last_state_change_unix_ms: i64,
    cooldown_duration: Duration,
) -> Duration {
    let elapsed_ms = u64::try_from(now_unix_ms.saturating_sub(last_state_change_unix_ms).max(0))
        .unwrap_or(u64::MAX);
    cooldown_duration.saturating_sub(Duration::from_millis(elapsed_ms))
}

#[must_use]
pub fn state_from_db_value(state_value: i16) -> CircuitState {
    match state_value {
        1 => CircuitState::HalfOpen,
        2 => CircuitState::Open,
        _ => CircuitState::Closed,
    }
}
