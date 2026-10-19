//! In-process rate limiter for connector probe / live runs (FR-W11).

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
    pub fn new(max_per_window: u32, window: Duration) -> Self {
        Self {
            max_per_window: max_per_window.max(1),
            window,
            events: Mutex::new(VecDeque::new()),
        }
    }

    /// Returns Ok(()) if allowed, Err remaining-wait hint otherwise.
    pub fn try_acquire(&self) -> Result<(), Duration> {
        let now = Instant::now();
        let mut q = self.events.lock().unwrap_or_else(|e| e.into_inner());
        while let Some(front) = q.front() {
            if now.duration_since(*front) > self.window {
                q.pop_front();
            } else {
                break;
            }
        }
        if q.len() as u32 >= self.max_per_window {
            let wait = q
                .front()
                .map(|t| self.window.saturating_sub(now.duration_since(*t)))
                .unwrap_or(self.window);
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
