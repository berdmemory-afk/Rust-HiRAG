//! Authentication and authorization for protocol messages

use crate::error::{ProtocolError, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

/// Maximum message age in seconds (5 minutes)
const MAX_MESSAGE_AGE: i64 = 300;

/// Authentication configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Shared secret for HMAC
    pub secret: String,
    
    /// Enable timestamp validation
    pub validate_timestamp: bool,
    
    /// Maximum message age in seconds
    pub max_age_secs: i64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            secret: String::new(),
            validate_timestamp: true,
            max_age_secs: MAX_MESSAGE_AGE,
        }
    }
}

/// Generate HMAC signature for message
pub fn generate_signature(secret: &str, message_id: &str, timestamp: i64, sender: &str) -> Result<String> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| ProtocolError::ValidationFailed(format!("Invalid secret: {}", e)))?;
    
    // Create signature payload
    let payload = format!("{}:{}:{}", message_id, timestamp, sender);
    mac.update(payload.as_bytes());
    
    // Get signature as hex string
    let result = mac.finalize();
    let signature = hex::encode(result.into_bytes());
    
    Ok(signature)
}

/// Verify HMAC signature for message
pub fn verify_signature(
    secret: &str,
    message_id: &str,
    timestamp: i64,
    sender: &str,
    signature: &str,
) -> Result<()> {
    let expected = generate_signature(secret, message_id, timestamp, sender)?;
    
    if signature != expected {
        return Err(ProtocolError::ValidationFailed("Invalid signature".to_string()).into());
    }
    
    Ok(())
}

/// Validate message timestamp
pub fn validate_timestamp(timestamp: i64, max_age_secs: i64) -> Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| ProtocolError::ValidationFailed(format!("System time error: {}", e)))?
        .as_secs() as i64;
    
    let age = now - timestamp;
    
    if age < 0 {
        return Err(ProtocolError::ValidationFailed("Message timestamp is in the future".to_string()).into());
    }
    
    if age > max_age_secs {
        return Err(ProtocolError::ValidationFailed(format!(
            "Message too old: {} seconds (max: {})",
            age, max_age_secs
        )).into());
    }
    
    Ok(())
}

/// Authenticate a message
pub fn authenticate_message(
    config: &AuthConfig,
    message_id: &str,
    timestamp: i64,
    sender: &str,
    signature: Option<&str>,
) -> Result<()> {
    // Validate timestamp if enabled
    if config.validate_timestamp {
        validate_timestamp(timestamp, config.max_age_secs)?;
    }
    
    // Verify signature if provided
    if let Some(sig) = signature {
        verify_signature(&config.secret, message_id, timestamp, sender, sig)?;
    } else if !config.secret.is_empty() {
        return Err(ProtocolError::ValidationFailed("Missing signature".to_string()).into());
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_signature_generation() {
        let secret = "test_secret";
        let message_id = "test_id";
        let timestamp = 1234567890;
        let sender = "test_sender";
        
        let sig1 = generate_signature(secret, message_id, timestamp, sender).unwrap();
        let sig2 = generate_signature(secret, message_id, timestamp, sender).unwrap();
        
        // Same inputs should produce same signature
        assert_eq!(sig1, sig2);
    }
    
    #[test]
    fn test_signature_verification() {
        let secret = "test_secret";
        let message_id = "test_id";
        let timestamp = 1234567890;
        let sender = "test_sender";
        
        let signature = generate_signature(secret, message_id, timestamp, sender).unwrap();
        
        // Valid signature should verify
        assert!(verify_signature(secret, message_id, timestamp, sender, &signature).is_ok());
        
        // Invalid signature should fail
        assert!(verify_signature(secret, message_id, timestamp, sender, "invalid").is_err());
    }
    
    #[test]
    fn test_timestamp_validation() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        
        // Current timestamp should be valid
        assert!(validate_timestamp(now, MAX_MESSAGE_AGE).is_ok());
        
        // Old timestamp should fail
        assert!(validate_timestamp(now - MAX_MESSAGE_AGE - 1, MAX_MESSAGE_AGE).is_err());
        
        // Future timestamp should fail
        assert!(validate_timestamp(now + 100, MAX_MESSAGE_AGE).is_err());
    }
    
    #[test]
    fn test_authenticate_message() {
        let config = AuthConfig {
            secret: "test_secret".to_string(),
            validate_timestamp: false, // Disable for testing
            max_age_secs: MAX_MESSAGE_AGE,
        };
        
        let message_id = "test_id";
        let timestamp = 1234567890;
        let sender = "test_sender";
        
        let signature = generate_signature(&config.secret, message_id, timestamp, sender).unwrap();
        
        // Valid authentication should succeed
        assert!(authenticate_message(&config, message_id, timestamp, sender, Some(&signature)).is_ok());
        
        // Invalid signature should fail
        assert!(authenticate_message(&config, message_id, timestamp, sender, Some("invalid")).is_err());
        
        // Missing signature should fail when secret is set
        assert!(authenticate_message(&config, message_id, timestamp, sender, None).is_err());
    }
}