//! Authentication middleware

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Authentication configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Valid API tokens
    pub valid_tokens: HashSet<String>,
    /// Whether authentication is enabled
    pub enabled: bool,
    /// Token prefix (e.g., "Bearer")
    pub token_prefix: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            valid_tokens: HashSet::new(),
            enabled: true,
            token_prefix: "Bearer".to_string(),
        }
    }
}

/// Authentication middleware
pub struct AuthMiddleware {
    config: Arc<RwLock<AuthConfig>>,
}

impl AuthMiddleware {
    /// Create new authentication middleware
    pub fn new(config: AuthConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Authenticate a request with token
    pub async fn authenticate(&self, token: &str) -> Result<(), AuthError> {
        let config = self.config.read().await;
        
        if !config.enabled {
            return Ok(());
        }

        // Remove prefix if present
        let token = if token.starts_with(&format!("{} ", config.token_prefix)) {
            token.trim_start_matches(&format!("{} ", config.token_prefix))
        } else {
            token
        };

        if config.valid_tokens.contains(token) {
            debug!("Authentication successful");
            Ok(())
        } else {
            warn!("Authentication failed: invalid token");
            Err(AuthError::InvalidToken)
        }
    }

    /// Add a valid token
    pub async fn add_token(&self, token: String) {
        let mut config = self.config.write().await;
        config.valid_tokens.insert(token);
        debug!("Token added to valid tokens");
    }

    /// Remove a token
    pub async fn remove_token(&self, token: &str) {
        let mut config = self.config.write().await;
        config.valid_tokens.remove(token);
        debug!("Token removed from valid tokens");
    }

    /// Check if token exists
    pub async fn has_token(&self, token: &str) -> bool {
        let config = self.config.read().await;
        config.valid_tokens.contains(token)
    }
    
    /// Validate token (synchronous version for middleware)
    pub fn validate_token(&self, token: &str) -> bool {
        // Use try_read to avoid blocking
        if let Ok(config) = self.config.try_read() {
            if !config.enabled {
                return true;
            }
            
            let token = if token.starts_with(&format!("{} ", config.token_prefix)) {
                token.trim_start_matches(&format!("{} ", config.token_prefix))
            } else {
                token
            };
            
            config.valid_tokens.contains(token)
        } else {
            false
        }
    }

    /// Get number of valid tokens
    pub async fn token_count(&self) -> usize {
        let config = self.config.read().await;
        config.valid_tokens.len()
    }
}

/// Authentication errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid or missing authentication token")]
    InvalidToken,
    
    #[error("Authentication is required but no token provided")]
    MissingToken,
    
    #[error("Token has expired")]
    ExpiredToken,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_valid_token() {
        let mut config = AuthConfig::default();
        config.valid_tokens.insert("test-token-123".to_string());
        
        let auth = AuthMiddleware::new(config);
        assert!(auth.authenticate("test-token-123").await.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_token() {
        let config = AuthConfig::default();
        let auth = AuthMiddleware::new(config);
        
        assert!(auth.authenticate("invalid-token").await.is_err());
    }

    #[tokio::test]
    async fn test_token_with_prefix() {
        let mut config = AuthConfig::default();
        config.valid_tokens.insert("test-token-123".to_string());
        
        let auth = AuthMiddleware::new(config);
        assert!(auth.authenticate("Bearer test-token-123").await.is_ok());
    }

    #[tokio::test]
    async fn test_disabled_auth() {
        let config = AuthConfig {
            enabled: false,
            ..Default::default()
        };
        
        let auth = AuthMiddleware::new(config);
        assert!(auth.authenticate("any-token").await.is_ok());
    }

    #[tokio::test]
    async fn test_add_remove_token() {
        let config = AuthConfig::default();
        let auth = AuthMiddleware::new(config);
        
        // Add token
        auth.add_token("new-token".to_string()).await;
        assert!(auth.has_token("new-token").await);
        
        // Remove token
        auth.remove_token("new-token").await;
        assert!(!auth.has_token("new-token").await);
    }
}