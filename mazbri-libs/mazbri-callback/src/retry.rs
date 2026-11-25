use chrono::{DateTime, TimeDelta, Utc};
use mazbri_core::num::SaturatingFrom;

pub const MAX_CALLBACK_RETRIES: i32 = 10;
pub const MIN_BACKOFF_MULTIPLIER: f64 = 1.0;
pub const MAX_BACKOFF_MULTIPLIER: f64 = 10.0;
pub const MAX_RETRY_DELAY_SECS: i64 = 86_400;

const MIN_INITIAL_DELAY_SECS: u64 = 1;
const MAX_INITIAL_DELAY_SECS: u64 = 3_600;
const MAX_RETRY_ATTEMPT_EXP: u32 = 16;

#[must_use]
pub fn calculate_retry_delay_secs(attempt: u32, initial_delay: u64, multiplier: f64) -> i64 {
    let bounded_initial = initial_delay.clamp(MIN_INITIAL_DELAY_SECS, MAX_INITIAL_DELAY_SECS);
    let safe_multiplier = if multiplier.is_nan() || multiplier < MIN_BACKOFF_MULTIPLIER {
        MIN_BACKOFF_MULTIPLIER
    } else if multiplier > MAX_BACKOFF_MULTIPLIER {
        MAX_BACKOFF_MULTIPLIER
    } else {
        multiplier
    };
    let bounded_attempt = i32::saturating_from(attempt.min(MAX_RETRY_ATTEMPT_EXP));
    let raw_delay = mazbri_core::num::f64_to_i64(
        mazbri_core::num::u64_to_f64(bounded_initial) * safe_multiplier.powi(bounded_attempt),
    );
    raw_delay.clamp(1, MAX_RETRY_DELAY_SECS)
}

#[must_use]
pub fn calculate_next_retry_from(
    now: DateTime<Utc>,
    attempt: u32,
    initial_delay: u64,
    multiplier: f64,
) -> DateTime<Utc> {
    let delay_secs = calculate_retry_delay_secs(attempt, initial_delay, multiplier);
    let delta = TimeDelta::try_seconds(delay_secs).unwrap_or_else(|| {
        TimeDelta::try_seconds(MAX_RETRY_DELAY_SECS).unwrap_or_else(TimeDelta::zero)
    });
    now + delta
}

#[must_use]
pub fn calculate_next_retry(attempt: u32, initial_delay: u64, multiplier: f64) -> DateTime<Utc> {
    calculate_next_retry_from(Utc::now(), attempt, initial_delay, multiplier)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_BACKOFF_MULTIPLIER, MAX_CALLBACK_RETRIES, MAX_RETRY_DELAY_SECS, MIN_BACKOFF_MULTIPLIER,
        calculate_next_retry, calculate_next_retry_from, calculate_retry_delay_secs,
    };
    use chrono::{TimeDelta, Utc};

    #[test]
    fn extreme_attempt_and_multiplier_cap_at_twenty_four_hours_without_panic() {
        let fixed_now = Utc::now();
        let next = calculate_next_retry_from(fixed_now, 100, 30, 100.0);
        assert_eq!(
            next,
            fixed_now + TimeDelta::try_seconds(MAX_RETRY_DELAY_SECS).expect("valid delta")
        );

        let before = Utc::now();
        let live_next = calculate_next_retry(100, 30, 100.0);
        let after = Utc::now();
        let max_delta = TimeDelta::try_seconds(MAX_RETRY_DELAY_SECS).expect("valid delta");
        assert!(live_next >= before + max_delta);
        assert!(live_next <= after + max_delta);
    }

    #[test]
    fn clamps_nan_inf_negative_and_zero_inputs_safely() {
        assert_eq!(MAX_CALLBACK_RETRIES, 10);
        assert!((MIN_BACKOFF_MULTIPLIER - 1.0).abs() < f64::EPSILON);
        assert!((MAX_BACKOFF_MULTIPLIER - 10.0).abs() < f64::EPSILON);

        assert_eq!(calculate_retry_delay_secs(3, 30, f64::NAN), 30);
        assert_eq!(calculate_retry_delay_secs(3, 30, -5.0), 30);
        assert_eq!(calculate_retry_delay_secs(3, 30, f64::NEG_INFINITY), 30);
        assert_eq!(
            calculate_retry_delay_secs(10, 30, f64::INFINITY),
            MAX_RETRY_DELAY_SECS
        );
        assert_eq!(calculate_retry_delay_secs(0, 0, 2.0), 1);
        assert_eq!(calculate_retry_delay_secs(0, u64::MAX, 1.0), 3_600);
        assert_eq!(calculate_retry_delay_secs(2, 30, 2.0), 120);
    }
}
