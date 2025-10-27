//! Rate limiting middleware for API protection

use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Rate limit configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window
    pub max_requests: usize,
    /// Time window duration
    pub window_duration: Duration,
    /// Whether to enable rate limiting
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_duration: Duration::from_secs(60),
            enabled: true,
        }
    }
}

/// Request record for tracking
#[derive(Debug, Clone)]
struct RequestRecord {
    count: usize,
    window_start: Instant,
}

/// Rate limiter implementation with lock-free DashMap
pub struct RateLimiter {
    config: RateLimitConfig,
    records: Arc<DashMap<String, RequestRecord>>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            records: Arc::new(DashMap::new()),
        }
    }

    /// Check if request should be allowed (lock-free)
    pub async fn check_rate_limit(&self, client_id: &str) -> Result<(), RateLimitError> {
        if !self.config.enabled {
            return Ok(());
        }

        let now = Instant::now();
        let client_key = client_id.to_string();

        // Get or create record
        let mut entry = self.records.entry(client_key.clone()).or_insert(RequestRecord {
            count: 0,
            window_start: now,
        });

        let record = entry.value_mut();

        // Check if window has expired
        if now.duration_since(record.window_start) >= self.config.window_duration {
            // Reset window
            record.count = 0;
            record.window_start = now;
        }

        // Check rate limit
        if record.count >= self.config.max_requests {
            let retry_after = self.config.window_duration
                .saturating_sub(now.duration_since(record.window_start));
            
            warn!(
                "Rate limit exceeded for client: {} ({} requests in window)",
                client_id, record.count
            );
            
            return Err(RateLimitError::LimitExceeded {
                retry_after,
                limit: self.config.max_requests,
            });
        }

        // Increment counter
        record.count += 1;
        debug!("Request allowed for client: {} ({}/{})", client_id, record.count, self.config.max_requests);

        Ok(())
    }

    /// Get current usage for a client
    pub async fn get_usage(&self, client_id: &str) -> Option<(usize, Duration)> {
        self.records.get(client_id).map(|record| {
            let elapsed = Instant::now().duration_since(record.window_start);
            (record.count, elapsed)
        })
    }

    /// Reset rate limit for a client
    pub async fn reset(&self, client_id: &str) {
        self.records.remove(client_id);
        debug!("Rate limit reset for client: {}", client_id);
    }

    /// Clean up expired records
    pub async fn cleanup_expired(&self) {
        let now = Instant::now();
        
        self.records.retain(|_, record| {
            now.duration_since(record.window_start) < self.config.window_duration
        });
        
        debug!("Cleaned up expired rate limit records");
    }
    
    /// Start background cleanup task
    pub fn start_cleanup_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.config.window_duration);
            loop {
                interval.tick().await;
                self.cleanup_expired().await;
            }
        })
    }

    /// Get statistics
    pub async fn stats(&self) -> RateLimitStats {
        let total_clients = self.records.len();
        let total_requests: usize = self.records.iter().map(|r| r.value().count).sum();
        
        RateLimitStats {
            total_clients,
            total_requests,
            config: self.config.clone(),
        }
    }
}

/// Rate limit error
#[derive(Debug, Clone, thiserror::Error)]
pub enum RateLimitError {
    #[error("Rate limit exceeded. Retry after {retry_after:?}. Limit: {limit} requests per window")]
    LimitExceeded {
        retry_after: Duration,
        limit: usize,
    },
}

/// Rate limit statistics
#[derive(Debug, Clone)]
pub struct RateLimitStats {
    pub total_clients: usize,
    pub total_requests: usize,
    pub config: RateLimitConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limit_allows_requests() {
        let config = RateLimitConfig {
            max_requests: 5,
            window_duration: Duration::from_secs(60),
            enabled: true,
        };
        let limiter = RateLimiter::new(config);

        for i in 0..5 {
            assert!(limiter.check_rate_limit("client1").await.is_ok(), "Request {} should be allowed", i);
        }
    }

    #[tokio::test]
    async fn test_rate_limit_blocks_excess() {
        let config = RateLimitConfig {
            max_requests: 3,
            window_duration: Duration::from_secs(60),
            enabled: true,
        };
        let limiter = RateLimiter::new(config);

        // Allow first 3 requests
        for _ in 0..3 {
            assert!(limiter.check_rate_limit("client1").await.is_ok());
        }

        // Block 4th request
        assert!(limiter.check_rate_limit("client1").await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limit_window_reset() {
        let config = RateLimitConfig {
            max_requests: 2,
            window_duration: Duration::from_millis(100),
            enabled: true,
        };
        let limiter = RateLimiter::new(config);

        // Use up limit
        limiter.check_rate_limit("client1").await.unwrap();
        limiter.check_rate_limit("client1").await.unwrap();
        assert!(limiter.check_rate_limit("client1").await.is_err());

        // Wait for window to expire
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should allow requests again
        assert!(limiter.check_rate_limit("client1").await.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limit_per_client() {
        let config = RateLimitConfig {
            max_requests: 2,
            window_duration: Duration::from_secs(60),
            enabled: true,
        };
        let limiter = RateLimiter::new(config);

        // Client 1 uses limit
        limiter.check_rate_limit("client1").await.unwrap();
        limiter.check_rate_limit("client1").await.unwrap();
        assert!(limiter.check_rate_limit("client1").await.is_err());

        // Client 2 should still be allowed
        assert!(limiter.check_rate_limit("client2").await.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limit_disabled() {
        let config = RateLimitConfig {
            max_requests: 1,
            window_duration: Duration::from_secs(60),
            enabled: false,
        };
        let limiter = RateLimiter::new(config);

        // Should allow unlimited requests when disabled
        for _ in 0..10 {
            assert!(limiter.check_rate_limit("client1").await.is_ok());
        }
    }
}