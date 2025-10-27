//! Embedding client for Chutes API

use super::{EmbeddingProvider, EmbeddingCache, models::*};
use crate::config::EmbeddingConfig;
use crate::error::{EmbeddingError, Result};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use secrecy::ExposeSecret;

/// Client for generating embeddings via Chutes API
pub struct EmbeddingClient {
    config: EmbeddingConfig,
    http_client: Client,
    cache: Option<Arc<EmbeddingCache>>,
}

impl EmbeddingClient {
    /// Create a new embedding client
    pub fn new(config: EmbeddingConfig) -> Result<Self> {
        // Build HTTP client with TLS verification enforcement
        let client_builder = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .pool_max_idle_per_host(10);
        
        // Enforce TLS verification in release mode
        #[cfg(not(debug_assertions))]
        {
            if config.tls_enabled && !config.tls_verify {
                return Err(EmbeddingError::ApiError(
                    "TLS verification cannot be disabled in release mode".to_string()
                ).into());
            }
        }
        
        let http_client = client_builder
            .build()
            .map_err(EmbeddingError::NetworkError)?;

        let cache = if config.cache_enabled {
            Some(Arc::new(EmbeddingCache::new(
                config.cache_size,
                Duration::from_secs(config.cache_ttl_secs),
            )))
        } else {
            None
        };

        info!("Initialized embedding client with cache_enabled={}", config.cache_enabled);
        
        Ok(Self {
            config,
            http_client,
            cache,
        })
    }
    
    /// Create client with custom HTTP client
    pub fn with_http_client(config: EmbeddingConfig, http_client: Client) -> Result<Self> {
        let cache = if config.cache_enabled {
            Some(Arc::new(EmbeddingCache::new(
                config.cache_size,
                Duration::from_secs(config.cache_ttl_secs),
            )))
        } else {
            None
        };
        
        Ok(Self {
            config,
            http_client,
            cache,
        })
    }
    
    /// Enable caching with specified cache
    pub fn with_cache(mut self, cache: Arc<EmbeddingCache>) -> Self {
        self.cache = Some(cache);
        self
    }
    
    /// Generate cache key for text
    fn cache_key(&self, text: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        format!("emb_{:x}", hasher.finish())
    }
    
    /// Make API request with retry logic
    async fn make_request(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse> {
        let mut attempts = 0;
        let mut last_error = None;
        
        while attempts < self.config.max_retries {
            attempts += 1;
            
            match self.try_request(request).await {
                Ok(response) => {
                    debug!("Embedding request succeeded on attempt {}", attempts);
                    return Ok(response);
                }
                Err(e) => {
                    warn!("Embedding request failed on attempt {}: {}", attempts, e);
                    last_error = Some(e);
                    
                    if attempts < self.config.max_retries {
                        // Exponential backoff with jitter
                        let base_delay = 100 * 2_u64.pow(attempts - 1);
                        let max_delay = 30_000; // Cap at 30 seconds
                        let delay = base_delay.min(max_delay);
                        
                        // Add jitter (±25%)
                        let jitter = (delay as f64 * 0.25 * (rand::random::<f64>() - 0.5)) as u64;
                        let final_delay = Duration::from_millis((delay as i64 + jitter as i64).max(0) as u64);
                        
                        debug!("Retrying after {}ms", final_delay.as_millis());
                        tokio::time::sleep(final_delay).await;
                    }
                }
            }
        }
        
        Err(last_error.unwrap())
    }
    
    /// Try to make a single API request
    async fn try_request(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse> {
        debug!("Making embedding API request");
        
        let response = self.http_client
            .post(&self.config.api_url)
            .header("Authorization", format!("Bearer {}", self.config.api_token.expose_secret()))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(EmbeddingError::NetworkError)?;
        
        let status = response.status();
        
        match status {
            StatusCode::OK => {
                let embedding_response: EmbeddingResponse = response
                    .json()
                    .await
                    .map_err(EmbeddingError::NetworkError)?;
                
                debug!("Received {} embeddings", embedding_response.data.len());
                Ok(embedding_response)
            }
            StatusCode::UNAUTHORIZED => {
                error!("Authentication failed");
                Err(EmbeddingError::AuthenticationFailed.into())
            }
            StatusCode::TOO_MANY_REQUESTS => {
                warn!("Rate limit exceeded");
                Err(EmbeddingError::RateLimitExceeded.into())
            }
            _ => {
                let error_text = response.text().await.unwrap_or_default();
                error!("API request failed with status {}: {}", status, error_text);
                Err(EmbeddingError::ApiError(format!(
                    "Status {}: {}", 
                    status, error_text
                )).into())
            }
        }
    }
}

#[async_trait]
impl EmbeddingProvider for EmbeddingClient {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        if text.is_empty() {
            return Err(EmbeddingError::InvalidInput("Text cannot be empty".to_string()).into());
        }
        
        if text.len() > 8192 {
            return Err(EmbeddingError::InvalidInput(
                format!("Text too long: {} characters (max 8192)", text.len())
            ).into());
        }
        
        // Check cache first
        if let Some(cache) = &self.cache {
            let key = self.cache_key(text);
            if let Some(embedding) = cache.get(&key).await {
                debug!("Cache hit for embedding");
                return Ok(embedding);
            }
        }
        
        // Make API request
        let request = EmbeddingRequest::single(text);
        let response = self.make_request(&request).await?;
        
        if response.data.is_empty() {
            return Err(EmbeddingError::ApiError("No embeddings returned".to_string()).into());
        }
        
        let embedding = response.data[0].embedding.clone();
        
        // Cache the result
        if let Some(cache) = &self.cache {
            let key = self.cache_key(text);
            cache.put(key, embedding.clone()).await;
        }
        
        Ok(embedding)
    }
    
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Err(EmbeddingError::InvalidInput("Texts cannot be empty".to_string()).into());
        }
        
        if texts.len() > self.config.batch_size {
            return Err(EmbeddingError::InvalidInput(
                format!("Batch size {} exceeds maximum {}", texts.len(), self.config.batch_size)
            ).into());
        }
        
        // Check cache for all texts
        let mut results = Vec::with_capacity(texts.len());
        let mut uncached_indices = Vec::new();
        let mut uncached_texts = Vec::new();
        
        if let Some(cache) = &self.cache {
            for (i, text) in texts.iter().enumerate() {
                let key = self.cache_key(text);
                if let Some(embedding) = cache.get(&key).await {
                    results.push(Some(embedding));
                } else {
                    results.push(None);
                    uncached_indices.push(i);
                    uncached_texts.push(text.clone());
                }
            }
        } else {
            uncached_texts = texts.to_vec();
            uncached_indices = (0..texts.len()).collect();
            results = vec![None; texts.len()];
        }
        
        // Fetch uncached embeddings
        if !uncached_texts.is_empty() {
            debug!("Fetching {} uncached embeddings", uncached_texts.len());
            
            let request = EmbeddingRequest::batch(uncached_texts.clone());
            let response = self.make_request(&request).await?;
            
            // Cache and store results
            for (i, embedding_data) in response.data.iter().enumerate() {
                let original_index = uncached_indices[i];
                let embedding = embedding_data.embedding.clone();
                
                if let Some(cache) = &self.cache {
                    let key = self.cache_key(&uncached_texts[i]);
                    cache.put(key, embedding.clone()).await;
                }
                
                results[original_index] = Some(embedding);
            }
        }
        
        // Convert Option<Vec<f32>> to Vec<f32>
        results.into_iter()
            .map(|opt| opt.ok_or_else(|| 
                EmbeddingError::ApiError("Missing embedding in response".to_string()).into()
            ))
            .collect()
    }
    
    fn embedding_dimension(&self) -> usize {
        1024 // intfloat/multilingual-e5-large produces 1024-dimensional embeddings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_key_generation() {
        let config = EmbeddingConfig {
            api_url: "http://test".to_string(),
            api_token: secrecy::Secret::new("test".to_string()),
            batch_size: 32,
            timeout_secs: 30,
            max_retries: 3,
            cache_enabled: false,
            cache_ttl_secs: 3600,
            cache_size: 1000,
            tls_enabled: false,
            tls_verify: true,
        };
        
        let client = EmbeddingClient::new(config).unwrap();
        let key1 = client.cache_key("test");
        let key2 = client.cache_key("test");
        let key3 = client.cache_key("different");
        
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }
}