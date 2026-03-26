use slg_core::errors::SlgError;
use std::time::Instant;

/// Token bucket rate limiter: 60 requests/minute, refill 1 token/second.
pub struct RateLimiter {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(max_rpm: u32) -> Self {
        let max = max_rpm as f64;
        Self {
            tokens: max,
            max_tokens: max,
            refill_rate: max / 60.0, // tokens per second
            last_refill: Instant::now(),
        }
    }

    /// Check if a request is allowed. Consumes 1 token if allowed.
    pub fn check(&mut self) -> Result<(), SlgError> {
        self.refill();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            Err(SlgError::RateLimitExceeded)
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_requests() {
        let mut limiter = RateLimiter::new(60);
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_ok());
    }

    #[test]
    fn test_rate_limiter_exhaustion() {
        let mut limiter = RateLimiter::new(2);
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_ok());
        // Third should fail (only 2 tokens)
        assert!(limiter.check().is_err());
    }
}
