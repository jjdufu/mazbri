use chrono::{DateTime, Duration, Utc};
pub fn calculate_next_retry(attempt: u32, initial_delay: u64, multiplier: f64) -> DateTime<Utc> {
    let delay = (initial_delay as f64 * multiplier.powi(attempt as i32)) as i64;
    Utc::now() + Duration::seconds(delay)
}
