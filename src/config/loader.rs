//! Configuration loader with environment variable support

use super::Config;
use crate::error::{ContextError, Result};
use config::{Environment, File};
use std::path::Path;
use secrecy::ExposeSecret;

/// Load configuration from a TOML file
pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config> {
    let config = config::Config::builder()
        .add_source(File::from(path.as_ref()))
        .build()?;
    
    let cfg: Config = config.try_deserialize()?;
    validate_config(&cfg)?;
    Ok(cfg)
}

/// Load configuration from a TOML file with environment variable overrides
pub fn load_config_with_env<P: AsRef<Path>>(path: P) -> Result<Config> {
    let config = config::Config::builder()
        .add_source(File::from(path.as_ref()))
        .add_source(
            Environment::with_prefix("CONTEXT_MANAGER")
                .separator("__")
                .try_parsing(true)
        )
        .build()?;
    
    let cfg: Config = config.try_deserialize()?;
    validate_config(&cfg)?;
    Ok(cfg)
}

/// Validate configuration values
fn validate_config(config: &Config) -> Result<()> {
    // Validate embedding config
    if config.embedding.api_token.expose_secret().is_empty() {
        return Err(ContextError::Config(
            "Embedding API token is required".to_string()
        ));
    }
    
    if config.embedding.batch_size == 0 {
        return Err(ContextError::Config(
            "Embedding batch size must be greater than 0".to_string()
        ));
    }
    
    // Validate vector DB config
    if config.vector_db.url.is_empty() {
        return Err(ContextError::Config(
            "Vector database URL is required".to_string()
        ));
    }
    
    if config.vector_db.vector_size == 0 {
        return Err(ContextError::Config(
            "Vector size must be greater than 0".to_string()
        ));
    }
    
    // Validate HiRAG config
    if config.hirag.max_context_tokens == 0 {
        return Err(ContextError::Config(
            "Max context tokens must be greater than 0".to_string()
        ));
    }
    
    let total_allocation = config.hirag.retrieval_strategy.l1_allocation
        + config.hirag.retrieval_strategy.l2_allocation
        + config.hirag.retrieval_strategy.l3_allocation;
    
    if (total_allocation - 1.0).abs() > 0.01 {
        return Err(ContextError::Config(
            format!("Retrieval strategy allocations must sum to 1.0, got {}", total_allocation)
        ));
    }
    
    // Validate ranking weights
    let total_weight = config.hirag.ranking_weights.similarity_weight
        + config.hirag.ranking_weights.recency_weight
        + config.hirag.ranking_weights.level_weight
        + config.hirag.ranking_weights.frequency_weight;
    
    if (total_weight - 1.0).abs() > 0.01 {
        return Err(ContextError::Config(
            format!("Ranking weights must sum to 1.0, got {}", total_weight)
        ));
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validate_config() {
        let mut config = Config::default_config();
        config.embedding.api_token = secrecy::Secret::new("test_token".to_string());
        
        assert!(validate_config(&config).is_ok());
    }
    
    #[test]
    fn test_validate_empty_token() {
        let mut config = Config::default_config();
        config.embedding.api_token = secrecy::Secret::new("".to_string());
        
        assert!(validate_config(&config).is_err());
    }
}