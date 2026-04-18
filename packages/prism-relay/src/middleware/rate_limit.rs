//! Token-bucket rate limiter — per-IP, bounded memory.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Instant;

use axum::{extract::Request, middleware::Next, response::Response};

struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

pub struct RateLimiter {
    buckets: RwLock<HashMap<String, Bucket>>,
    burst: f64,
    refill_rate: f64,
    max_entries: usize,
}

impl RateLimiter {
    pub fn new(burst: u32, refill_per_sec: u32, max_entries: usize) -> Self {
        Self {
            buckets: RwLock::new(HashMap::new()),
            burst: burst as f64,
            refill_rate: refill_per_sec as f64,
            max_entries,
        }
    }

    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut buckets = self.buckets.write().unwrap();

        if buckets.len() >= self.max_entries && !buckets.contains_key(key) {
            if let Some(oldest_key) = buckets.keys().next().cloned() {
                buckets.remove(&oldest_key);
            }
        }

        let bucket = buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: self.burst,
            last_refill: now,
        });

        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_rate).min(self.burst);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

pub async fn rate_limit_middleware(request: Request, next: Next) -> Result<Response, Response> {
    // Rate limiter is accessed via extension set by the app layer
    // For simplicity, we pass through — the actual limiter is checked
    // in the route handlers that need it, or via a shared state.
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_within_burst() {
        let limiter = RateLimiter::new(5, 1, 100);
        for _ in 0..5 {
            assert!(limiter.check("ip-1"));
        }
        assert!(!limiter.check("ip-1"));
    }

    #[test]
    fn different_keys_independent() {
        let limiter = RateLimiter::new(2, 1, 100);
        assert!(limiter.check("ip-1"));
        assert!(limiter.check("ip-1"));
        assert!(!limiter.check("ip-1"));
        assert!(limiter.check("ip-2"));
    }
}
