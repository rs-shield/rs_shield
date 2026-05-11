use crate::auth::types::RateLimitCounter;
use chrono::{Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct RateLimiter {
    // user_id -> RateLimitCounter
    counters: Arc<RwLock<HashMap<String, RateLimitCounter>>>,
    max_requests: u32,
    window_seconds: i64,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_seconds: i64) -> Self {
        Self {
            counters: Arc::new(RwLock::new(HashMap::new())),
            max_requests,
            window_seconds,
        }
    }

    pub async fn is_allowed(&self, user_id: &str) -> bool {
        let mut counters = self.counters.write().await;
        let now = Utc::now();

        let counter = counters
            .entry(user_id.to_string())
            .or_insert_with(|| RateLimitCounter {
                user_id: user_id.to_string(),
                counter: 0,
                reset_at: now + Duration::seconds(self.window_seconds),
            });

        // Reset se janela expirou
        if now > counter.reset_at {
            counter.counter = 0;
            counter.reset_at = now + Duration::seconds(self.window_seconds);
        }

        if counter.counter < self.max_requests {
            counter.counter += 1;
            true
        } else {
            false
        }
    }

    pub async fn get_remaining(&self, user_id: &str) -> u32 {
        let counters = self.counters.read().await;
        let now = Utc::now();

        if let Some(counter) = counters.get(user_id) {
            if now > counter.reset_at {
                self.max_requests
            } else {
                self.max_requests.saturating_sub(counter.counter)
            }
        } else {
            self.max_requests
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limit_allows_requests() {
        let limiter = RateLimiter::new(5, 60);

        for _ in 0..5 {
            assert!(limiter.is_allowed("user1").await);
        }

        assert!(!limiter.is_allowed("user1").await);
    }

    #[tokio::test]
    async fn test_rate_limit_different_users() {
        let limiter = RateLimiter::new(2, 60);

        assert!(limiter.is_allowed("user1").await);
        assert!(limiter.is_allowed("user1").await);
        assert!(!limiter.is_allowed("user1").await);

        assert!(limiter.is_allowed("user2").await);
        assert!(limiter.is_allowed("user2").await);
        assert!(!limiter.is_allowed("user2").await);
    }
}
