//! 按键限频的日志闸门：同一个键在间隔内只放行一次，用于故障期高频告警日志降噪。

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// 记录的键数超过此值时，清理已过间隔的旧键，避免长期运行时无界增长。
const PRUNE_THRESHOLD: usize = 1024;

#[derive(Debug)]
pub struct RateLimitedLog {
    interval: Duration,
    last: Mutex<HashMap<String, Instant>>,
}

impl RateLimitedLog {
    #[must_use]
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: Mutex::new(HashMap::new()),
        }
    }

    /// 该键本次是否应当打日志；放行时记下时间。
    #[must_use]
    pub fn allow(&self, key: &str) -> bool {
        self.allow_at(key, Instant::now())
    }

    fn allow_at(&self, key: &str, now: Instant) -> bool {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(prev) = last.get(key)
            && now.saturating_duration_since(*prev) < self.interval
        {
            return false;
        }
        if last.len() >= PRUNE_THRESHOLD {
            let interval = self.interval;
            last.retain(|_, t| now.saturating_duration_since(*t) < interval);
        }
        last.insert(key.to_string(), now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_once_per_interval_per_key() {
        let log = RateLimitedLog::new(Duration::from_secs(10));
        let t0 = Instant::now();
        assert!(log.allow_at("a", t0));
        assert!(!log.allow_at("a", t0 + Duration::from_secs(9)));
        assert!(log.allow_at("b", t0 + Duration::from_secs(9)));
        assert!(log.allow_at("a", t0 + Duration::from_secs(10)));
    }

    #[test]
    fn prunes_expired_keys() {
        let log = RateLimitedLog::new(Duration::from_secs(1));
        let t0 = Instant::now();
        for i in 0..PRUNE_THRESHOLD {
            assert!(log.allow_at(&i.to_string(), t0));
        }
        assert!(log.allow_at("late", t0 + Duration::from_secs(2)));
        assert_eq!(log.last.lock().unwrap().len(), 1);
    }
}
