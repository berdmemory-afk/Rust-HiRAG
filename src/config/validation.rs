//! Configuration validation

use super::*;
use crate::error::{ContextError, Result};

/// Validate complete configuration
pub fn validate_config(config: &Config) -> Result<()> {
    validate_embedding_config(&config.embedding)?;
    validate_vector_db_config(&config.vector_db)?;
    validate_hirag_config(&config.hirag)?;
    validate_protocol_config(&config.protocol)?;
    validate_server_config(&config.server)?;
    Ok(())
}

/// Validate embedding configuration
fn validate_embedding_config(config: &EmbeddingConfig) -> Result<()> {
    // Validate API URL
    if config.api_url.is_empty() {
        return Err(ContextError::Config(
            "Embedding API URL cannot be empty".to_string()
        ));
    }
    
    if !config.api_url.starts_with("http://") && !config.api_url.starts_with("https://") {
        return Err(ContextError::Config(
            "Embedding API URL must start with http:// or https://".to_string()
        ));
    }
    
    // Validate API token
    if config.api_token.expose_secret().is_empty() {
        return Err(ContextError::Config(
            "Embedding API token is required".to_string()
        ));
    }
    
    // Validate batch size
    if config.batch_size == 0 {
        return Err(ContextError::Config(
            "Embedding batch size must be greater than 0".to_string()
        ));
    }
    
    if config.batch_size > 1000 {
        return Err(ContextError::Config(
            "Embedding batch size too large (max: 1000)".to_string()
        ));
    }
    
    // Validate timeout
    if config.timeout_secs == 0 {
        return Err(ContextError::Config(
            "Embedding timeout must be greater than 0".to_string()
        ));
    }
    
    if config.timeout_secs > 300 {
        return Err(ContextError::Config(
            "Embedding timeout too large (max: 300 seconds)".to_string()
        ));
    }
    
    // Validate max retries
    if config.max_retries > 10 {
        return Err(ContextError::Config(
            "Max retries too large (max: 10)".to_string()
        ));
    }
    
    // Validate cache settings
    if config.cache_enabled {
        if config.cache_size == 0 {
            return Err(ContextError::Config(
                "Cache size must be greater than 0 when cache is enabled".to_string()
            ));
        }
        
        if config.cache_ttl_secs == 0 {
            return Err(ContextError::Config(
                "Cache TTL must be greater than 0 when cache is enabled".to_string()
            ));
        }
    }
    
    Ok(())
}

/// Validate vector database configuration
fn validate_vector_db_config(config: &VectorDbConfig) -> Result<()> {
    // Validate URL
    if config.url.is_empty() {
        return Err(ContextError::Config(
            "Vector database URL cannot be empty".to_string()
        ));
    }
    
    if !config.url.starts_with("http://") && !config.url.starts_with("https://") {
        return Err(ContextError::Config(
            "Vector database URL must start with http:// or https://".to_string()
        ));
    }
    
    // Validate TLS configuration
    if config.tls_enabled && !config.url.starts_with("https://") {
        return Err(ContextError::Config(
            "TLS is enabled but URL does not use https://".to_string()
        ));
    }
    
    // Validate collection prefix
    if config.collection_prefix.is_empty() {
        return Err(ContextError::Config(
            "Collection prefix cannot be empty".to_string()
        ));
    }
    
    // Validate vector size
    if config.vector_size == 0 {
        return Err(ContextError::Config(
            "Vector size must be greater than 0".to_string()
        ));
    }
    
    if config.vector_size > 4096 {
        return Err(ContextError::Config(
            "Vector size too large (max: 4096)".to_string()
        ));
    }
    
    // Validate timeout
    if config.timeout_secs == 0 {
        return Err(ContextError::Config(
            "Database timeout must be greater than 0".to_string()
        ));
    }
    
    if config.timeout_secs > 300 {
        return Err(ContextError::Config(
            "Database timeout too large (max: 300 seconds)".to_string()
        ));
    }
    
    // Fatal error if TLS verify disabled in release mode
    #[cfg(not(debug_assertions))]
    {
        if config.tls_enabled && !config.tls_verify {
            return Err(ContextError::Config(
                "TLS certificate verification cannot be disabled in production (release mode)".to_string()
            ));
        }
    }
    
    Ok(())
}

/// Validate HiRAG configuration
fn validate_hirag_config(config: &HiRAGConfig) -> Result<()> {
    // Validate L1 size
    if config.l1_size == 0 {
        return Err(ContextError::Config(
            "L1 cache size must be greater than 0".to_string()
        ));
    }
    
    if config.l1_size > 10000 {
        return Err(ContextError::Config(
            "L1 cache size too large (max: 10000)".to_string()
        ));
    }
    
    // Validate L2 size
    if config.l2_size == 0 {
        return Err(ContextError::Config(
            "L2 size must be greater than 0".to_string()
        ));
    }
    
    if config.l2_size > 1000000 {
        return Err(ContextError::Config(
            "L2 size too large (max: 1000000)".to_string()
        ));
    }
    
    // Validate max context tokens
    if config.max_context_tokens == 0 {
        return Err(ContextError::Config(
            "Max context tokens must be greater than 0".to_string()
        ));
    }
    
    if config.max_context_tokens > 1000000 {
        return Err(ContextError::Config(
            "Max context tokens too large (max: 1000000)".to_string()
        ));
    }
    
    // Validate relevance threshold
    if config.relevance_threshold < 0.0 || config.relevance_threshold > 1.0 {
        return Err(ContextError::Config(
            "Relevance threshold must be between 0.0 and 1.0".to_string()
        ));
    }
    
    // Validate ranking weights
    let weights = &config.ranking_weights;
    if weights.similarity_weight < 0.0 || weights.similarity_weight > 1.0 {
        return Err(ContextError::Config(
            "Similarity weight must be between 0.0 and 1.0".to_string()
        ));
    }
    
    if weights.recency_weight < 0.0 || weights.recency_weight > 1.0 {
        return Err(ContextError::Config(
            "Recency weight must be between 0.0 and 1.0".to_string()
        ));
    }
    
    if weights.level_weight < 0.0 || weights.level_weight > 1.0 {
        return Err(ContextError::Config(
            "Level weight must be between 0.0 and 1.0".to_string()
        ));
    }
    
    if weights.frequency_weight < 0.0 || weights.frequency_weight > 1.0 {
        return Err(ContextError::Config(
            "Frequency weight must be between 0.0 and 1.0".to_string()
        ));
    }
    
    // Weights should sum to approximately 1.0
    let sum = weights.similarity_weight + weights.recency_weight + weights.level_weight + weights.frequency_weight;
    if (sum - 1.0).abs() > 0.01 {
        return Err(ContextError::Config(
            format!("Ranking weights should sum to 1.0 (current sum: {:.2})", sum)
        ));
    }
    
    Ok(())
}

/// Validate protocol configuration
fn validate_protocol_config(config: &ProtocolConfig) -> Result<()> {
    // Validate version
    if config.version.is_empty() {
        return Err(ContextError::Config(
            "Protocol version cannot be empty".to_string()
        ));
    }
    
    // Validate max message size
    if config.max_message_size_mb == 0 {
        return Err(ContextError::Config(
            "Max message size must be greater than 0".to_string()
        ));
    }
    
    if config.max_message_size_mb > 100 {
        return Err(ContextError::Config(
            "Max message size too large (max: 100 MB)".to_string()
        ));
    }
    
    Ok(())
}

/// Validate server configuration
pub fn validate_server_config(config: &ServerConfig) -> Result<()> {
    // Validate port range
    if config.port == 0 {
        return Err(ContextError::Config(
            "Server port cannot be 0".to_string()
        ));
    }
    
    // Validate host
    if config.host.is_empty() {
        return Err(ContextError::Config(
            "Server host cannot be empty".to_string()
        ));
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::Secret;
    
    #[test]
    fn test_valid_config() {
        let config = Config::default_config();
        // This will fail because default config has empty token
        // Let's create a valid one
        let mut config = config;
        config.embedding.api_token = Secret::new("test_token".to_string());
        
        assert!(validate_config(&config).is_ok());
    }
    
    #[test]
    fn test_invalid_embedding_url() {
        let mut config = Config::default_config();
        config.embedding.api_token = Secret::new("test_token".to_string());
        config.embedding.api_url = "invalid_url".to_string();
        
        assert!(validate_embedding_config(&config.embedding).is_err());
    }
    
    #[test]
    fn test_invalid_batch_size() {
        let mut config = Config::default_config();
        config.embedding.api_token = Secret::new("test_token".to_string());
        config.embedding.batch_size = 0;
        
        assert!(validate_embedding_config(&config.embedding).is_err());
    }
    
    #[test]
    fn test_invalid_vector_size() {
        let mut config = Config::default_config();
        config.vector_db.vector_size = 0;
        
        assert!(validate_vector_db_config(&config.vector_db).is_err());
    }
    
    #[test]
    fn test_invalid_relevance_threshold() {
        let mut config = Config::default_config();
        config.hirag.relevance_threshold = 1.5;
        
        assert!(validate_hirag_config(&config.hirag).is_err());
    }
    
    #[test]
    fn test_invalid_ranking_weights() {
        let mut config = Config::default_config();
        config.hirag.ranking_weights.similarity_weight = 0.5;
        config.hirag.ranking_weights.recency_weight = 0.5;
        config.hirag.ranking_weights.level_weight = 0.5;
        config.hirag.ranking_weights.frequency_weight = 0.5; // Sum > 1.0
        
        assert!(validate_hirag_config(&config.hirag).is_err());
    }
}