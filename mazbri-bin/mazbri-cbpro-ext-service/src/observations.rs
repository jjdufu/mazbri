#[must_use]
pub fn now_unix_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
