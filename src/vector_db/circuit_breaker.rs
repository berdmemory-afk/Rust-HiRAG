//! Circuit breaker for vector database operations

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed, requests flow normally
    Closed,
    
    /// Circuit is open, requests are rejected
    Open,
    
    /// Circuit is half-open, testing if service recovered
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Failure threshold to open circuit
    pub failure_threshold: usize,
    
    /// Success threshold to close circuit from half-open
    pub success_threshold: usize,
    
    /// Timeout before attempting to close circuit
    pub timeout: Duration,
    
    /// Window size for counting failures
    pub window_size: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
            window_size: Duration::from_secs(60),
        }
    }
}

/// Circuit breaker for protecting against cascading failures
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<RwLock<CircuitState>>,
    failure_count: Arc<AtomicUsize>,
    success_count: Arc<AtomicUsize>,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    total_calls: Arc<AtomicU64>,
    total_failures: Arc<AtomicU64>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: Arc::new(AtomicUsize::new(0)),
            success_count: Arc::new(AtomicUsize::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            total_calls: Arc::new(AtomicU64::new(0)),
            total_failures: Arc::new(AtomicU64::new(0)),
        }
    }
    
    /// Check if request should be allowed
    pub async fn allow_request(&self) -> bool {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        
        let state = *self.state.read().await;
        
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                if let Some(last_failure) = *self.last_failure_time.read().await {
                    if last_failure.elapsed() >= self.config.timeout {
                        // Transition to half-open
                        *self.state.write().await = CircuitState::HalfOpen;
                        self.success_count.store(0, Ordering::Relaxed);
                        debug!("Circuit breaker transitioning to half-open state");
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }
    
    /// Record a successful operation
    pub async fn record_success(&self) {
        let state = *self.state.read().await;
        
        match state {
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count.store(0, Ordering::Relaxed);
            }
            CircuitState::HalfOpen => {
                let successes = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
                
                if successes >= self.config.success_threshold {
                    // Transition to closed
                    *self.state.write().await = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::Relaxed);
                    self.success_count.store(0, Ordering::Relaxed);
                    debug!("Circuit breaker closed after successful recovery");
                }
            }
            CircuitState::Open => {}
        }
    }
    
    /// Record a failed operation
    pub async fn record_failure(&self) {
        self.total_failures.fetch_add(1, Ordering::Relaxed);
        
        let state = *self.state.read().await;
        
        match state {
            CircuitState::Closed => {
                let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
                
                if failures >= self.config.failure_threshold {
                    // Transition to open
                    *self.state.write().await = CircuitState::Open;
                    *self.last_failure_time.write().await = Some(Instant::now());
                    warn!("Circuit breaker opened after {} failures", failures);
                }
            }
            CircuitState::HalfOpen => {
                // Transition back to open
                *self.state.write().await = CircuitState::Open;
                *self.last_failure_time.write().await = Some(Instant::now());
                self.success_count.store(0, Ordering::Relaxed);
                warn!("Circuit breaker reopened after failure in half-open state");
            }
            CircuitState::Open => {}
        }
    }
    
    /// Get current state
    pub async fn state(&self) -> CircuitState {
        *self.state.read().await
    }
    
    /// Get statistics
    pub async fn stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            state: *self.state.read().await,
            total_calls: self.total_calls.load(Ordering::Relaxed),
            total_failures: self.total_failures.load(Ordering::Relaxed),
            current_failures: self.failure_count.load(Ordering::Relaxed),
        }
    }
    
    /// Export circuit breaker state as Prometheus gauge (0=closed, 1=half-open, 2=open)
    pub async fn export_prometheus(&self, name: &str) -> String {
        let state_value = match *self.state.read().await {
            CircuitState::Closed => 0,
            CircuitState::HalfOpen => 1,
            CircuitState::Open => 2,
        };
        
        let stats = self.stats().await;
        
        format!(
            "# HELP {}_state Circuit breaker state (0=closed, 1=half-open, 2=open)\n\
             # TYPE {}_state gauge\n\
             {}_state {}\n\
             \n\
             # HELP {}_calls_total Total calls through circuit breaker\n\
             # TYPE {}_calls_total counter\n\
             {}_calls_total {}\n\
             \n\
             # HELP {}_failures_total Total failures\n\
             # TYPE {}_failures_total counter\n\
             {}_failures_total {}\n\
             \n\
             # HELP {}_current_failures Current failure count in window\n\
             # TYPE {}_current_failures gauge\n\
             {}_current_failures {}\n",
            name, name, name, state_value,
            name, name, name, stats.total_calls,
            name, name, name, stats.total_failures,
            name, name, name, stats.current_failures
        )
    }
    
    /// Reset circuit breaker
    pub async fn reset(&self) {
        *self.state.write().await = CircuitState::Closed;
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        *self.last_failure_time.write().await = None;
        debug!("Circuit breaker reset");
    }
}

/// Circuit breaker statistics
#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub total_calls: u64,
    pub total_failures: u64,
    pub current_failures: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_circuit_breaker_closed() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_secs(1),
            window_size: Duration::from_secs(60),
        };
        
        let cb = CircuitBreaker::new(config);
        
        assert_eq!(cb.state().await, CircuitState::Closed);
        assert!(cb.allow_request().await);
    }
    
    #[tokio::test]
    async fn test_circuit_breaker_opens() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_secs(1),
            window_size: Duration::from_secs(60),
        };
        
        let cb = CircuitBreaker::new(config);
        
        // Record failures
        cb.record_failure().await;
        cb.record_failure().await;
        cb.record_failure().await;
        
        assert_eq!(cb.state().await, CircuitState::Open);
        assert!(!cb.allow_request().await);
    }
    
    #[tokio::test]
    async fn test_circuit_breaker_half_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            window_size: Duration::from_secs(60),
        };
        
        let cb = CircuitBreaker::new(config);
        
        // Open circuit
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);
        
        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Should transition to half-open
        assert!(cb.allow_request().await);
        assert_eq!(cb.state().await, CircuitState::HalfOpen);
    }
    
    #[tokio::test]
    async fn test_circuit_breaker_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            window_size: Duration::from_secs(60),
        };
        
        let cb = CircuitBreaker::new(config);
        
        // Open circuit
        cb.record_failure().await;
        cb.record_failure().await;
        
        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Transition to half-open
        cb.allow_request().await;
        
        // Record successes
        cb.record_success().await;
        cb.record_success().await;
        
        // Should be closed now
        assert_eq!(cb.state().await, CircuitState::Closed);
    }
}