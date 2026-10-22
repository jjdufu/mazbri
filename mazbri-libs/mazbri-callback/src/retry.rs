use chrono::{DateTime, Duration, Utc};
use mazbri_core::num::SaturatingFrom;
#[must_use]
pub fn calculate_next_retry(attempt: u32, initial_delay: u64, multiplier: f64) -> DateTime<Utc> {
    let delay = mazbri_core::num::f64_to_i64(
        (mazbri_core::num::u64_to_f64(initial_delay)
            * multiplier.powi(i32::saturating_from(attempt))),
    );
    Utc::now() + Duration::seconds(delay)
}
