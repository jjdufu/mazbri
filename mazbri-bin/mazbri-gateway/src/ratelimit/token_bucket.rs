use std::time::Instant;

#[derive(Debug, Clone)]
pub struct TokenBucket {
    capacity: u64,
    refill_per_sec: f64,
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    pub fn new(capacity: u64, refill_per_sec: f64, now: Instant) -> Self {
        Self {
            capacity,
            refill_per_sec: refill_per_sec.max(0.0),
            tokens: mazbri_core::num::u64_to_f64(capacity),
            last_refill: now,
        }
    }

    pub fn reset(&mut self, capacity: u64, refill_per_sec: f64, now: Instant) {
        self.capacity = capacity;
        self.refill_per_sec = refill_per_sec.max(0.0);
        self.tokens = mazbri_core::num::u64_to_f64(capacity);
        self.last_refill = now;
    }

    fn refill(&mut self, now: Instant) {
        if now <= self.last_refill {
            return;
        }
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        if elapsed <= 0.0 || self.refill_per_sec <= 0.0 {
            self.last_refill = now;
            return;
        }
        self.tokens = (self.tokens + elapsed * self.refill_per_sec)
            .min(mazbri_core::num::u64_to_f64(self.capacity));
        self.last_refill = now;
    }

    pub fn try_consume(&mut self, amount: u64, now: Instant) -> bool {
        self.refill(now);
        if amount == 0 {
            return true;
        }
        // Single request larger than burst capacity can never pass.
        if amount > self.capacity {
            return false;
        }
        let amount_f = mazbri_core::num::u64_to_f64(amount);
        if self.tokens >= amount_f {
            self.tokens -= amount_f;
            true
        } else {
            false
        }
    }

    /// Best-effort debit used after a response is already committed (no tear-back).
    /// Returns how many units were actually removed from the bucket.
    pub fn consume_up_to(&mut self, amount: u64, now: Instant) -> u64 {
        self.refill(now);
        if amount == 0 || self.tokens <= 0.0 {
            return 0;
        }
        let take = mazbri_core::num::u64_to_f64(amount).min(self.tokens);
        self.tokens -= take;
        mazbri_core::num::f64_to_u64(take.floor())
    }

    pub fn retry_after_ms(&mut self, amount: u64, now: Instant) -> u64 {
        self.refill(now);
        if amount == 0 {
            return 0;
        }
        let amount_f = mazbri_core::num::u64_to_f64(amount);
        if self.tokens >= amount_f || self.refill_per_sec <= 0.0 {
            return 0;
        }
        // amount > capacity: wait until a full refill of capacity is still not enough;
        // report time to accumulate `amount` from current tokens (may be large).
        let need = amount_f - self.tokens;
        mazbri_core::num::f64_to_u64(((need / self.refill_per_sec) * 1000.0).ceil())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn reject_when_amount_exceeds_capacity() {
        let now = Instant::now();
        let mut bucket = TokenBucket::new(100, 100.0, now);
        assert!(!bucket.try_consume(101, now));
    }

    #[test]
    fn consume_up_to_drains_available_tokens() {
        let now = Instant::now();
        let mut bucket = TokenBucket::new(50, 10.0, now);
        let taken = bucket.consume_up_to(80, now);
        assert_eq!(taken, 50);
        assert!(!bucket.try_consume(1, now));
    }

    #[test]
    fn refill_restores_tokens() {
        let now = Instant::now();
        let mut bucket = TokenBucket::new(100, 100.0, now);
        assert!(bucket.try_consume(100, now));
        let later = now + Duration::from_secs(1);
        assert!(bucket.try_consume(100, later));
    }
}
