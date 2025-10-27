//! Enhanced embedding client with improved cache handling and error recovery

use super::{EmbeddingProvider, EmbeddingCache, models::*};
use crate::config::EmbeddingConfig;
use crate::error::{EmbeddingError, Result, ContextError};
use crate::middleware::InputValidator;
use crate::vector_db::{CircuitBreaker, CircuitBreakerConfig};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use secrecy::ExposeSecret;

/// Enhanced client for generating embeddings with improved concurrency
pub struct EmbeddingClientV2 {
    config: EmbeddingConfig,
    http_client: Client,
    cache: Option<Arc<EmbeddingCache>>,
    circuit_breaker: Option<Arc<CircuitBreaker>>,
}

impl EmbeddingClientV2 {
    /// Create a new embedding client
    pub fn new(config: EmbeddingConfig) -> Result<Self> {
        // Enforce TLS verification in release builds
        #[cfg(not(debug_assertions))]
        if config.tls_enabled && !config.tls_verify {
            return Err(ContextError::Embedding(
                EmbeddingError::ApiError(
                    "TLS verification cannot be disabled in release mode".to_string()
                )
            ));
        }
        
        let mut client_builder = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .pool_max_idle_per_host(10);
        
        // Enforce TLS settings
        if config.tls_enabled {
            client_builder = client_builder.https_only(true);
            
            // In release builds, enforce TLS verification if enabled
            #[cfg(not(debug_assertions))]
            if config.tls_verify {
                // This is the default behavior, but we're being explicit
                // No additional configuration needed as verification is enabled by default
            } else {
                // In debug builds, allow disabling verification if explicitly configured
                #[cfg(debug_assertions)]
                {
                    client_builder = client_builder.danger_accept_invalid_certs(true);
                }
            }
        }
        
        let http_client = client_builder
            .build()
            .map_err(|e| ContextError::Embedding(EmbeddingError::NetworkError(e)))?;
        
        let cache = if config.cache_enabled {
            Some(Arc::new(EmbeddingCache::new(
                config.cache_size,
                Duration::from_secs(config.cache_ttl_secs),
            )))
        } else {
            None
        };
        
        info!("Initialized enhanced embedding client with cache_enabled={}", config.cache_enabled);
        
        Ok(Self {
            config,
            http_client,
            cache,
            circuit_breaker: None,
        })
    }
    
    /// Enable circuit breaker protection
    pub fn with_circuit_breaker(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker = Some(Arc::new(CircuitBreaker::new(config)));
        info!("Circuit breaker enabled for embedding client");
        self
    }
    
    /// Create client with custom HTTP client
    pub fn with_http_client(config: EmbeddingConfig, http_client: Client) -> Result<Self> {
        // Enforce TLS verification in release builds
        #[cfg(not(debug_assertions))]
        if config.tls_enabled && !config.tls_verify {
            return Err(ContextError::Embedding(
                EmbeddingError::ApiError(
                    "TLS verification cannot be disabled in release mode".to_string()
                )
            ));
        }
        
        let cache = if config.cache_enabled {
            Some(Arc::new(EmbeddingCache::new(
                config.cache_size,
                Duration::from_secs(config.cache_ttl_secs),
            )))
        } else {
            None
        };
        
        // Warn about TLS verification in release builds if custom client is provided
        #[cfg(not(debug_assertions))]
        if config.tls_enabled && config.tls_verify {
            // Check if the client has TLS verification enabled
            // Note: This is a limitation of reqwest - we can't easily verify the client's TLS settings
            warn!("Using custom HTTP client - ensure TLS verification is properly configured");
        }
        
        Ok(Self {
            config,
            http_client,
            cache,
            circuit_breaker: None,
        })
    }
    
    /// Enable caching with specified cache
    pub fn with_cache(mut self, cache: Arc<EmbeddingCache>) -> Self {
        self.cache = Some(cache);
        self
    }
    
    /// Generate cache key for text using SHA-256
    fn cache_key(&self, text: &str) -> String {
        use sha2::{Sha256, Digest};
        
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        format!("emb_{:x}", hasher.finalize())
    }
    
    /// Make API request with retry logic and adaptive backoff
    async fn make_request(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse> {
        // Check circuit breaker first
        if let Some(cb) = &self.circuit_breaker {
            if !cb.allow_request().await {
                warn!("Circuit breaker is open, rejecting embedding request");
                return Err(ContextError::Embedding(
                    EmbeddingError::ServiceUnavailable("Circuit breaker open".to_string())
                ));
            }
        }
        
        let mut attempts = 0;
        let max_retries = self.config.max_retries;
        
        loop {
            attempts += 1;
            
            match self.http_client
                .post(&self.config.api_url)
                .bearer_auth(self.config.api_token.expose_secret())
                .json(request)
                .send()
                .await
            {
                Ok(response) => {
                    // Record success for circuit breaker
                    if let Some(cb) = &self.circuit_breaker {
                        cb.record_success().await;
                    }
                    
                    let status = response.status();
                    if status.is_success() {
                        match response.json::<EmbeddingResponse>().await {
                            Ok(embedding_response) => {
                                debug!("Embedding request successful after {} attempts", attempts);
                                return Ok(embedding_response);
                            }
                            Err(e) => {
                                error!("Failed to parse embedding response: {}", e);
                                // Record failure for circuit breaker
                                if let Some(cb) = &self.circuit_breaker {
                                    cb.record_failure().await;
                                }
                                
                                if attempts <= max_retries {
                                    let backoff = Duration::from_millis(100 * (2_u64.pow(attempts as u32)));
                                    debug!("Retrying embedding request in {:?}", backoff);
                                    tokio::time::sleep(backoff).await;
                                    continue;
                                }
                                
                                return Err(ContextError::Embedding(EmbeddingError::ApiError(format!("Failed to parse response: {}", e))));
                            }
                        }
                    } else {
                        // Record failure for circuit breaker
                        if let Some(cb) = &self.circuit_breaker {
                            cb.record_failure().await;
                        }
                        
                        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                        error!("Embedding API error {}: {}", status, error_text);
                        
                        match status {
                            StatusCode::TOO_MANY_REQUESTS => {
                                if attempts <= max_retries {
                                    // Exponential backoff with jitter for rate limiting
                                    let backoff = Duration::from_millis(500 * (2_u64.pow(attempts as u32)));
                                    let jitter = Duration::from_millis(rand::random::<u64>() % 1000);
                                    let total_backoff = backoff + jitter;
                                    debug!("Rate limited, retrying in {:?}", total_backoff);
                                    tokio::time::sleep(total_backoff).await;
                                    continue;
                                }
                            }
                            StatusCode::UNAUTHORIZED => {
                                return Err(ContextError::Embedding(EmbeddingError::AuthenticationFailed));
                            }
                            _ => {
                                if attempts <= max_retries {
                                    let backoff = Duration::from_millis(100 * (2_u64.pow(attempts as u32)));
                                    debug!("Retrying embedding request in {:?}", backoff);
                                    tokio::time::sleep(backoff).await;
                                    continue;
                                }
                            }
                        }
                        
                        return Err(ContextError::Embedding(EmbeddingError::ApiError(format!("API error {}: {}", status, error_text))));
                    }
                }
                Err(e) => {
                    // Record failure for circuit breaker
                    if let Some(cb) = &self.circuit_breaker {
                        cb.record_failure().await;
                    }
                    
                    error!("Network error during embedding request: {}", e);
                    
                    if attempts <= max_retries {
                        let backoff = Duration::from_millis(100 * (2_u64.pow(attempts as u32)));
                        debug!("Retrying embedding request in {:?}", backoff);
                        tokio::time::sleep(backoff).await;
                        continue;
                    }
                    
                    return Err(ContextError::Embedding(EmbeddingError::NetworkError(e)));
                }
            }
        }
    }
}

#[async_trait]
impl EmbeddingProvider for EmbeddingClientV2 {
    /// Generate embedding for a single text
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        // Validate input
        InputValidator::validate_text(text)?;
        
        // Check cache first
        if let Some(cache) = &self.cache {
            if let Some(cached) = cache.get(text).await {
                debug!("Cache hit for embedding");
                return Ok(cached);
            }
        }
        
        // Generate cache key
        let cache_key = self.cache_key(text);
        
        // Prepare request
        let request = EmbeddingRequest::single(text.to_string());
        
        // Make request
        let response = self.make_request(&request).await?;
        
        // Extract embedding
        let embedding = response
            .data
            .into_iter()
            .next()
            .ok_or_else(|| ContextError::Embedding(EmbeddingError::ApiError("No embedding in response".to_string())))?
            .embedding;
        
        // Store in cache
        if let Some(cache) = &self.cache {
            cache.put(cache_key, embedding.clone()).await;
        }
        
        Ok(embedding)
    }
    
    /// Generate embeddings for multiple texts
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // Validate inputs
        for text in texts {
            InputValidator::validate_text(text)?;
        }
        
        // Process in batches
        let mut results = Vec::new();
        let batch_size = self.config.batch_size;
        
        for chunk in texts.chunks(batch_size) {
            // Check cache for all items in batch
            let mut batch_results = Vec::new();
            let mut uncached_texts = Vec::new();
            let mut uncached_indices = Vec::new();
            
            if let Some(cache) = &self.cache {
                for (i, text) in chunk.iter().enumerate() {
                    if let Some(cached) = cache.get(text).await {
                        batch_results.push((i, cached));
                    } else {
                        uncached_texts.push(text.clone());
                        uncached_indices.push(i);
                    }
                }
            } else {
                uncached_texts = chunk.to_vec();
                uncached_indices = (0..chunk.len()).collect();
            }
            
            // Generate embeddings for uncached texts
            if !uncached_texts.is_empty() {
                let request = EmbeddingRequest::batch(uncached_texts.clone());
                
                let response = self.make_request(&request).await?;
                
                // Extract embeddings and store in cache
                for (i, embedding_data) in response.data.into_iter().enumerate() {
                    let embedding = embedding_data.embedding;
                    if let Some(cache) = &self.cache {
                        cache.put(self.cache_key(&uncached_texts[i]), embedding.clone()).await;
                    }
                    batch_results.push((uncached_indices[i], embedding));
                }
            }
            
            // Sort results by index and add to final results
            batch_results.sort_by_key(|(index, _)| *index);
            results.extend(batch_results.into_iter().map(|(_, embedding)| embedding));
        }
        
        Ok(results)
    }
    
    /// Get the dimension of embeddings
    fn embedding_dimension(&self) -> usize {
        // multilingual-e5-large has 1024 dimensions
        1024
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::Secret;
    
    #[tokio::test]
    async fn test_cache_key_generation() {
        let config = EmbeddingConfig {
            api_url: "https://api.example.com".to_string(),
            api_token: Secret::new("test-token".to_string()),
            batch_size: 32,
            timeout_secs: 30,
            max_retries: 3,
            cache_enabled: true,
            cache_ttl_secs: 3600,
            cache_size: 1000,
            tls_enabled: false,
            tls_verify: true,
        };
        
        let client = EmbeddingClientV2::new(config).unwrap();
        let key1 = client.cache_key("test text");
        let key2 = client.cache_key("test text");
        let key3 = client.cache_key("different text");
        
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }
}