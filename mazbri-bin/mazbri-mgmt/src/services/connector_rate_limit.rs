//! In-process rate limiter for connector probe / live runs (FR-W11).

use mazbri_core::num::SaturatingFrom;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Sliding-window limiter: max `max_per_window` events per `window`.
#[derive(Debug)]
pub struct SlidingWindowLimiter {
    max_per_window: u32,
    window: Duration,
    events: Mutex<VecDeque<Instant>>,
}

impl SlidingWindowLimiter {
    #[must_use]
    pub fn new(max_per_window: u32, window: Duration) -> Self {
        Self {
            max_per_window: max_per_window.max(1),
            window,
            events: Mutex::new(VecDeque::new()),
        }
    }

    /// Returns Ok(()) if allowed, Err remaining-wait hint otherwise.
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn try_acquire(&self) -> Result<(), Duration> {
        let now = Instant::now();
        let mut q = self
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while let Some(front) = q.front() {
            if now.duration_since(*front) > self.window {
                q.pop_front();
            } else {
                break;
            }
        }
        if u32::saturating_from(q.len()) >= self.max_per_window {
            let wait = q.front().map_or(self.window, |t| {
                self.window.saturating_sub(now.duration_since(*t))
            });
            return Err(wait);
        }
        q.push_back(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_then_blocks() {
        let lim = SlidingWindowLimiter::new(2, Duration::from_secs(60));
        assert!(lim.try_acquire().is_ok());
        assert!(lim.try_acquire().is_ok());
        assert!(lim.try_acquire().is_err());
    }
}
