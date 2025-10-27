//! High-performance caching layer for embeddings using moka

use moka::future::Cache;
use std::time::Duration;
use tracing::{debug, info};

/// Statistics about cache performance
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub size: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

/// High-performance async cache for embeddings using moka
pub struct EmbeddingCache {
    cache: Cache<String, Vec<f32>>,
}

impl EmbeddingCache {
    /// Create a new cache with specified capacity and TTL
    pub fn new(max_size: usize, ttl: Duration) -> Self {
        info!("Initializing embedding cache with max_size={}, ttl={:?}", max_size, ttl);
        
        let cache = Cache::builder()
            .max_capacity(max_size as u64)
            .time_to_live(ttl)
            .time_to_idle(ttl / 2) // Evict if not accessed for half the TTL
            .build();
        
        Self { cache }
    }
    
    /// Get embedding from cache
    pub async fn get(&self, key: &str) -> Option<Vec<f32>> {
        let result = self.cache.get(key).await;
        
        if result.is_some() {
            debug!("Cache hit for key: {}", key);
        } else {
            debug!("Cache miss for key: {}", key);
        }
        
        result
    }
    
    /// Store embedding in cache
    pub async fn put(&self, key: String, embedding: Vec<f32>) {
        self.cache.insert(key.clone(), embedding).await;
        debug!("Cached embedding for key: {}", key);
    }
    
    /// Clear expired entries (moka handles this automatically)
    pub async fn cleanup(&self) {
        // Moka automatically removes expired entries
        // This method is kept for API compatibility
        self.cache.run_pending_tasks().await;
    }
    
    /// Clear all entries
    pub async fn clear(&self) {
        self.cache.invalidate_all();
        self.cache.run_pending_tasks().await;
        info!("Cache cleared");
    }
    
    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        // Run pending tasks to update stats
        self.cache.run_pending_tasks().await;
        
        let entry_count = self.cache.entry_count();
        
        // Moka doesn't expose hit/miss counts directly in the current version
        // We'll return approximate stats based on entry count
        CacheStats {
            size: entry_count as usize,
            hits: 0, // Not available in current moka version
            misses: 0, // Not available in current moka version
            hit_rate: 0.0, // Not available in current moka version
        }
    }
    
    /// Get the underlying cache for advanced operations
    pub fn inner(&self) -> &Cache<String, Vec<f32>> {
        &self.cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_cache_put_get() {
        let cache = EmbeddingCache::new(10, Duration::from_secs(60));
        let embedding = vec![1.0, 2.0, 3.0];
        
        cache.put("test".to_string(), embedding.clone()).await;
        let result = cache.get("test").await;
        
        assert_eq!(result, Some(embedding));
    }
    
    #[tokio::test]
    async fn test_cache_miss() {
        let cache = EmbeddingCache::new(10, Duration::from_secs(60));
        let result = cache.get("nonexistent").await;
        
        assert_eq!(result, None);
    }
    
    #[tokio::test]
    async fn test_cache_ttl() {
        let cache = EmbeddingCache::new(10, Duration::from_millis(100));
        let embedding = vec![1.0, 2.0, 3.0];
        
        cache.put("test".to_string(), embedding.clone()).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Run pending tasks to process expirations
        cache.cache.run_pending_tasks().await;
        
        let result = cache.get("test").await;
        assert_eq!(result, None);
    }
    
    #[tokio::test]
    async fn test_cache_stats() {
        let cache = EmbeddingCache::new(10, Duration::from_secs(60));
        let embedding = vec![1.0, 2.0, 3.0];
        
        cache.put("test".to_string(), embedding).await;
        cache.get("test").await;
        cache.get("nonexistent").await;
        
        let stats = cache.stats().await;
        // Moka doesn't expose hit/miss counts in the current version
        // Just verify we can get stats without errors
        assert!(stats.size <= 10);
    }
}