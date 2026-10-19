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
pub fn instant_from_db_timestamp(name: &str, db_last_change: Option<DateTime<Utc>>) -> Instant {
    match db_last_change {
        Some(ts) => {
            let elapsed = Utc::now() - ts;
            match elapsed.to_std() {
                Ok(duration) => Instant::now() - duration,
                Err(_) => {
                    warn!(
                        circuit_breaker = % name, db_ts = % ts,
                        "last_state_change is in the future (clock skew), treating as just transitioned"
                    );
                    Instant::now()
                }
            }
        }
        None => Instant::now(),
    }
}
pub fn build_cache_entry(state: CircuitState, last_state_change: Instant) -> LocalCacheEntry {
    LocalCacheEntry {
        state,
        last_state_change,
        cached_at: Instant::now(),
    }
}
