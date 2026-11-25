use super::LocalCacheEntry;
use crate::CircuitState;
use chrono::{DateTime, Utc};
use sqlx::Row;
use std::time::Instant;
use tracing::warn;
pub fn get_row_field_i64(row: &sqlx::postgres::PgRow, field: &str, context: &str) -> i64 {
    match row.try_get::<i64, _>(field) {
        Ok(val) => val,
        Err(e) => {
            warn!(
                field = field, error = % e, context = context,
                "Database row missing expected field, using default 0"
            );
            0
        }
    }
}
pub fn get_row_field_i16(row: &sqlx::postgres::PgRow, field: &str, context: &str) -> i16 {
    match row.try_get::<i16, _>(field) {
        Ok(val) => val,
        Err(e) => {
            warn!(
                field = field, error = % e, context = context,
                "Database row missing expected field, using default 0"
            );
            0
        }
    }
}
pub fn unix_ms_from_db_timestamp(name: &str, db_last_change: Option<DateTime<Utc>>) -> i64 {
    let now_ms = Utc::now().timestamp_millis();
    match db_last_change {
        Some(ts) => {
            let ts_ms = ts.timestamp_millis();
            if ts_ms > now_ms {
                warn!(
                    circuit_breaker = %name,
                    db_ts = %ts,
                    "last_state_change is in the future (clock skew), treating as just transitioned"
                );
                now_ms
            } else {
                ts_ms
            }
        }
        None => now_ms,
    }
}

#[cfg(test)]
pub fn build_cache_entry(state: CircuitState, last_state_change_unix_ms: i64) -> LocalCacheEntry {
    build_cache_entry_with_failures(state, last_state_change_unix_ms, 0)
}

pub fn build_cache_entry_with_failures(
    state: CircuitState,
    last_state_change_unix_ms: i64,
    failure_count: u64,
) -> LocalCacheEntry {
    LocalCacheEntry {
        state,
        failure_count,
        last_state_change_unix_ms,
        fetched_at: Instant::now(),
    }
}
