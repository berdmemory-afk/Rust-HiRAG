//! Health check endpoints and monitoring

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::debug;

/// Health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Component health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// Component name
    pub name: String,
    
    /// Health status
    pub status: HealthStatus,
    
    /// Optional message
    pub message: Option<String>,
    
    /// Response time in milliseconds
    pub response_time_ms: Option<u64>,
}

/// Overall system health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    /// Overall status
    pub status: HealthStatus,
    
    /// Uptime in seconds
    pub uptime_secs: u64,
    
    /// Component health checks
    pub components: Vec<ComponentHealth>,
    
    /// Timestamp
    pub timestamp: i64,
}

/// Cached health check result
#[derive(Debug, Clone)]
struct CachedHealth {
    result: SystemHealth,
    cached_at: Instant,
}

/// Health checker with caching
pub struct HealthChecker {
    start_time: Instant,
    vector_db: Option<std::sync::Arc<dyn crate::vector_db::VectorStore>>,
    embedding_client: Option<std::sync::Arc<dyn crate::embedding::EmbeddingProvider>>,
    cache: Option<std::sync::Arc<crate::embedding::EmbeddingCache>>,
    circuit_breaker: Option<std::sync::Arc<crate::vector_db::CircuitBreaker>>,
    cached_result: Arc<RwLock<Option<CachedHealth>>>,
    cache_ttl: Duration,
}

impl HealthChecker {
    /// Create a new health checker with default 30-second cache TTL
    pub fn new() -> Self {
        Self::with_cache_ttl(Duration::from_secs(30))
    }
    
    /// Create a new health checker with custom cache TTL
    pub fn with_cache_ttl(cache_ttl: Duration) -> Self {
        Self {
            start_time: Instant::now(),
            vector_db: None,
            embedding_client: None,
            cache: None,
            circuit_breaker: None,
            cached_result: Arc::new(RwLock::new(None)),
            cache_ttl,
        }
    }
    
    /// Set vector database for health checks
    pub fn with_vector_db(mut self, vector_db: std::sync::Arc<dyn crate::vector_db::VectorStore>) -> Self {
        self.vector_db = Some(vector_db);
        self
    }
    
    /// Set embedding client for health checks
    pub fn with_embedding_client(mut self, embedding_client: std::sync::Arc<dyn crate::embedding::EmbeddingProvider>) -> Self {
        self.embedding_client = Some(embedding_client);
        self
    }
    
    /// Set cache for health checks
    pub fn with_cache(mut self, cache: std::sync::Arc<crate::embedding::EmbeddingCache>) -> Self {
        self.cache = Some(cache);
        self
    }
    
    /// Set circuit breaker for health checks
    pub fn with_circuit_breaker(mut self, circuit_breaker: std::sync::Arc<crate::vector_db::CircuitBreaker>) -> Self {
        self.circuit_breaker = Some(circuit_breaker);
        self
    }
    
    /// Check overall system health with caching
    pub async fn check_health(&self) -> SystemHealth {
        // Check if we have a valid cached result
        {
            let cached = self.cached_result.read().await;
            if let Some(cached_health) = &*cached {
                if cached_health.cached_at.elapsed() < self.cache_ttl {
                    debug!("Returning cached health check result");
                    return cached_health.result.clone();
                }
            }
        }
        
        // Perform actual health check
        debug!("Performing fresh health check");
        let health = self.perform_health_check().await;
        
        // Cache the result
        {
            let mut cached = self.cached_result.write().await;
            *cached = Some(CachedHealth {
                result: health.clone(),
                cached_at: Instant::now(),
            });
        }
        
        health
    }
    
    /// Perform actual health check (uncached)
    async fn perform_health_check(&self) -> SystemHealth {
        let mut components = Vec::new();
        
        // Check embedding service
        components.push(self.check_embedding_service().await);
        
        // Check vector database
        components.push(self.check_vector_db().await);
        
        // Check cache
        components.push(self.check_cache().await);
        
        // Check circuit breaker
        components.push(self.check_circuit_breaker().await);
        
        // Determine overall status
        let status = if components.iter().all(|c| c.status == HealthStatus::Healthy) {
            HealthStatus::Healthy
        } else if components.iter().any(|c| c.status == HealthStatus::Unhealthy) {
            HealthStatus::Unhealthy
        } else {
            HealthStatus::Degraded
        };
        
        SystemHealth {
            status,
            uptime_secs: self.start_time.elapsed().as_secs(),
            components,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
    
    /// Force refresh health check (bypass cache)
    pub async fn check_health_fresh(&self) -> SystemHealth {
        let health = self.perform_health_check().await;
        
        // Update cache
        let mut cached = self.cached_result.write().await;
        *cached = Some(CachedHealth {
            result: health.clone(),
            cached_at: Instant::now(),
        });
        
        health
    }
    
    /// Check embedding service health
    async fn check_embedding_service(&self) -> ComponentHealth {
        let start = Instant::now();
        
        if let Some(client) = &self.embedding_client {
            // Try to get embedding dimension as a health check
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                async { client.embedding_dimension() }
            ).await {
                Ok(dim) if dim > 0 => ComponentHealth {
                    name: "embedding_service".to_string(),
                    status: HealthStatus::Healthy,
                    message: Some(format!("Service operational (dim: {})", dim)),
                    response_time_ms: Some(start.elapsed().as_millis() as u64),
                },
                Ok(_) => ComponentHealth {
                    name: "embedding_service".to_string(),
                    status: HealthStatus::Unhealthy,
                    message: Some("Invalid embedding dimension".to_string()),
                    response_time_ms: Some(start.elapsed().as_millis() as u64),
                },
                Err(_) => ComponentHealth {
                    name: "embedding_service".to_string(),
                    status: HealthStatus::Unhealthy,
                    message: Some("Health check timeout".to_string()),
                    response_time_ms: Some(5000),
                },
            }
        } else {
            ComponentHealth {
                name: "embedding_service".to_string(),
                status: HealthStatus::Degraded,
                message: Some("Not configured".to_string()),
                response_time_ms: None,
            }
        }
    }
    
    /// Check vector database health
    async fn check_vector_db(&self) -> ComponentHealth {
        let start = Instant::now();
        
        if let Some(db) = &self.vector_db {
            // Try a simple query as a health check (get non-existent point)
            let test_id = uuid::Uuid::nil();
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                db.get_point("health_check", test_id)
            ).await {
                Ok(Ok(_)) => ComponentHealth {
                    name: "vector_database".to_string(),
                    status: HealthStatus::Healthy,
                    message: Some("Database operational".to_string()),
                    response_time_ms: Some(start.elapsed().as_millis() as u64),
                },
                Ok(Err(e)) => {
                    // Some errors are expected (e.g., collection doesn't exist)
                    // Only mark as unhealthy for connection errors
                    let error_msg = e.to_string();
                    let is_connection_error = error_msg.contains("connection") || error_msg.contains("timeout");
                    let status = if is_connection_error {
                        HealthStatus::Unhealthy
                    } else {
                        HealthStatus::Healthy // Collection not existing is fine for health check
                    };
                    let message = if is_connection_error {
                        format!("Database error: {}", error_msg)
                    } else {
                        "Database operational".to_string()
                    };
                    ComponentHealth {
                        name: "vector_database".to_string(),
                        status,
                        message: Some(message),
                        response_time_ms: Some(start.elapsed().as_millis() as u64),
                    }
                },
                Err(_) => ComponentHealth {
                    name: "vector_database".to_string(),
                    status: HealthStatus::Unhealthy,
                    message: Some("Health check timeout".to_string()),
                    response_time_ms: Some(5000),
                },
            }
        } else {
            ComponentHealth {
                name: "vector_database".to_string(),
                status: HealthStatus::Degraded,
                message: Some("Not configured".to_string()),
                response_time_ms: None,
            }
        }
    }
    
    /// Check cache health
    async fn check_cache(&self) -> ComponentHealth {
        let start = Instant::now();
        
        if let Some(cache) = &self.cache {
            // Get cache statistics
            let stats = cache.stats().await;
            let hit_rate = if stats.hits + stats.misses > 0 {
                stats.hits as f64 / (stats.hits + stats.misses) as f64
            } else {
                0.0
            };
            
            let status = if hit_rate > 0.5 {
                HealthStatus::Healthy
            } else {
                HealthStatus::Degraded // Low hit rate but not critical
            };
            
            ComponentHealth {
                name: "cache".to_string(),
                status,
                message: Some(format!(
                    "Cache operational (hit rate: {:.1}%, size: {})",
                    hit_rate * 100.0,
                    stats.size
                )),
                response_time_ms: Some(start.elapsed().as_millis() as u64),
            }
        } else {
            ComponentHealth {
                name: "cache".to_string(),
                status: HealthStatus::Degraded,
                message: Some("Not configured".to_string()),
                response_time_ms: None,
            }
        }
    }
    
    /// Check circuit breaker health
    async fn check_circuit_breaker(&self) -> ComponentHealth {
        if let Some(cb) = &self.circuit_breaker {
            let state = cb.state().await;
            let (status, message) = match state {
                crate::vector_db::circuit_breaker::CircuitState::Closed => {
                    (HealthStatus::Healthy, "Circuit closed - normal operation")
                }
                crate::vector_db::circuit_breaker::CircuitState::Open => {
                    (HealthStatus::Unhealthy, "Circuit open - service unavailable")
                }
                crate::vector_db::circuit_breaker::CircuitState::HalfOpen => {
                    (HealthStatus::Degraded, "Circuit half-open - testing recovery")
                }
            };
            
            ComponentHealth {
                name: "circuit_breaker".to_string(),
                status,
                message: Some(message.to_string()),
                response_time_ms: Some(0),
            }
        } else {
            ComponentHealth {
                name: "circuit_breaker".to_string(),
                status: HealthStatus::Degraded,
                message: Some("Not configured".to_string()),
                response_time_ms: None,
            }
        }
    }
    
    /// Simple liveness check
    pub fn liveness(&self) -> bool {
        true
    }
    
    /// Readiness check
    pub async fn readiness(&self) -> bool {
        let health = self.check_health().await;
        health.status != HealthStatus::Unhealthy
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_health_check() {
        let checker = HealthChecker::new();
        let health = checker.check_health().await;
        
        // Without configured components, status should be Degraded
        assert_eq!(health.status, HealthStatus::Degraded);
        assert!(!health.components.is_empty());
        assert_eq!(health.components.len(), 4); // embedding, vector_db, cache, circuit_breaker
    }
    
    #[test]
    fn test_liveness() {
        let checker = HealthChecker::new();
        assert!(checker.liveness());
    }
    
    #[tokio::test]
    async fn test_readiness() {
        let checker = HealthChecker::new();
        assert!(checker.readiness().await);
    }
}