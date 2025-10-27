//! Input validation middleware

use tracing::{debug, warn};

/// Maximum text length (8KB)
const MAX_TEXT_LENGTH: usize = 8192;

/// Maximum batch size
const MAX_BATCH_SIZE: usize = 100;

/// Input validator
pub struct InputValidator;

impl InputValidator {
    /// Validate text input
    pub fn validate_text(text: &str) -> Result<(), ValidationError> {
        // Check if empty
        if text.trim().is_empty() {
            warn!("Validation failed: empty text");
            return Err(ValidationError::EmptyInput);
        }

        // Check length
        if text.len() > MAX_TEXT_LENGTH {
            warn!("Validation failed: text too long ({} > {})", text.len(), MAX_TEXT_LENGTH);
            return Err(ValidationError::TextTooLong {
                length: text.len(),
                max_length: MAX_TEXT_LENGTH,
            });
        }

        // Check for control characters (except whitespace)
        if text.chars().any(|c| c.is_control() && !c.is_whitespace()) {
            warn!("Validation failed: contains control characters");
            return Err(ValidationError::InvalidCharacters);
        }

        debug!("Text validation passed");
        Ok(())
    }

    /// Sanitize text input
    pub fn sanitize_text(text: &str) -> String {
        text.chars()
            .filter(|c| !c.is_control() || c.is_whitespace())
            .collect::<String>()
            .trim()
            .to_string()
    }

    /// Validate batch size
    pub fn validate_batch_size(size: usize) -> Result<(), ValidationError> {
        if size == 0 {
            warn!("Validation failed: empty batch");
            return Err(ValidationError::EmptyBatch);
        }

        if size > MAX_BATCH_SIZE {
            warn!("Validation failed: batch too large ({} > {})", size, MAX_BATCH_SIZE);
            return Err(ValidationError::BatchTooLarge {
                size,
                max_size: MAX_BATCH_SIZE,
            });
        }

        debug!("Batch size validation passed");
        Ok(())
    }

    /// Validate token count
    pub fn validate_token_count(count: usize, max: usize) -> Result<(), ValidationError> {
        if count == 0 {
            warn!("Validation failed: zero token count");
            return Err(ValidationError::InvalidTokenCount);
        }

        if count > max {
            warn!("Validation failed: token count exceeds maximum ({} > {})", count, max);
            return Err(ValidationError::TokenLimitExceeded {
                count,
                max,
            });
        }

        debug!("Token count validation passed");
        Ok(())
    }

    /// Validate vector dimension
    pub fn validate_vector_dimension(actual: usize, expected: usize) -> Result<(), ValidationError> {
        if actual != expected {
            warn!("Validation failed: invalid vector dimension ({} != {})", actual, expected);
            return Err(ValidationError::InvalidVectorDimension {
                actual,
                expected,
            });
        }

        debug!("Vector dimension validation passed");
        Ok(())
    }

    /// Validate relevance score
    pub fn validate_relevance_score(score: f32) -> Result<(), ValidationError> {
        if !(0.0..=1.0).contains(&score) {
            warn!("Validation failed: invalid relevance score ({})", score);
            return Err(ValidationError::InvalidRelevanceScore { score });
        }

        debug!("Relevance score validation passed");
        Ok(())
    }

    /// Validate metadata key
    pub fn validate_metadata_key(key: &str) -> Result<(), ValidationError> {
        if key.is_empty() {
            return Err(ValidationError::EmptyMetadataKey);
        }

        if key.len() > 256 {
            return Err(ValidationError::MetadataKeyTooLong {
                length: key.len(),
                max_length: 256,
            });
        }

        // Check for invalid characters
        if !key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err(ValidationError::InvalidMetadataKey);
        }

        Ok(())
    }
    
    /// Validate metadata value
    pub fn validate_metadata_value(value: &serde_json::Value) -> Result<(), ValidationError> {
        // Serialize to check size
        let serialized = serde_json::to_string(value)
            .map_err(|_| ValidationError::InvalidMetadataValue)?;
        
        // Limit individual value size to 16KB
        if serialized.len() > 16 * 1024 {
            return Err(ValidationError::MetadataValueTooLarge {
                size: serialized.len(),
                max_size: 16 * 1024,
            });
        }
        
        // Check for potentially dangerous content in strings
        if let serde_json::Value::String(s) = value {
            // Check for null bytes
            if s.contains('\0') {
                return Err(ValidationError::InvalidMetadataValue);
            }
            
            // Check for control characters (except newline, tab, carriage return)
            if s.chars().any(|c| c.is_control() && c != '\n' && c != '\t' && c != '\r') {
                return Err(ValidationError::InvalidMetadataValue);
            }
        }
        
        Ok(())
    }
}

/// Validation errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum ValidationError {
    #[error("Input text is empty")]
    EmptyInput,

    #[error("Text too long: {length} characters (max: {max_length})")]
    TextTooLong { length: usize, max_length: usize },

    #[error("Text contains invalid control characters")]
    InvalidCharacters,

    #[error("Batch is empty")]
    EmptyBatch,

    #[error("Batch too large: {size} items (max: {max_size})")]
    BatchTooLarge { size: usize, max_size: usize },

    #[error("Invalid token count")]
    InvalidTokenCount,

    #[error("Token limit exceeded: {count} (max: {max})")]
    TokenLimitExceeded { count: usize, max: usize },

    #[error("Invalid vector dimension: {actual} (expected: {expected})")]
    InvalidVectorDimension { actual: usize, expected: usize },

    #[error("Invalid relevance score: {score} (must be between 0.0 and 1.0)")]
    InvalidRelevanceScore { score: f32 },

    #[error("Metadata key is empty")]
    EmptyMetadataKey,

    #[error("Metadata key too long: {length} (max: {max_length})")]
    MetadataKeyTooLong { length: usize, max_length: usize },

    #[error("Invalid metadata key (must contain only alphanumeric, underscore, or hyphen)")]
    InvalidMetadataKey,
    
    #[error("Invalid metadata value")]
    InvalidMetadataValue,
    
    #[error("Metadata value too large: {size} bytes (max: {max_size})")]
    MetadataValueTooLarge { size: usize, max_size: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_text_success() {
        assert!(InputValidator::validate_text("Hello, world!").is_ok());
    }

    #[test]
    fn test_validate_text_empty() {
        assert!(InputValidator::validate_text("").is_err());
        assert!(InputValidator::validate_text("   ").is_err());
    }

    #[test]
    fn test_validate_text_too_long() {
        let long_text = "a".repeat(MAX_TEXT_LENGTH + 1);
        assert!(InputValidator::validate_text(&long_text).is_err());
    }

    #[test]
    fn test_sanitize_text() {
        let text = "Hello\x00World\x01!";
        let sanitized = InputValidator::sanitize_text(text);
        assert_eq!(sanitized, "HelloWorld!");
    }

    #[test]
    fn test_validate_batch_size() {
        assert!(InputValidator::validate_batch_size(50).is_ok());
        assert!(InputValidator::validate_batch_size(0).is_err());
        assert!(InputValidator::validate_batch_size(MAX_BATCH_SIZE + 1).is_err());
    }

    #[test]
    fn test_validate_token_count() {
        assert!(InputValidator::validate_token_count(100, 1000).is_ok());
        assert!(InputValidator::validate_token_count(0, 1000).is_err());
        assert!(InputValidator::validate_token_count(1001, 1000).is_err());
    }

    #[test]
    fn test_validate_vector_dimension() {
        assert!(InputValidator::validate_vector_dimension(1024, 1024).is_ok());
        assert!(InputValidator::validate_vector_dimension(512, 1024).is_err());
    }

    #[test]
    fn test_validate_relevance_score() {
        assert!(InputValidator::validate_relevance_score(0.5).is_ok());
        assert!(InputValidator::validate_relevance_score(0.0).is_ok());
        assert!(InputValidator::validate_relevance_score(1.0).is_ok());
        assert!(InputValidator::validate_relevance_score(-0.1).is_err());
        assert!(InputValidator::validate_relevance_score(1.1).is_err());
    }

    #[test]
    fn test_validate_metadata_key() {
        assert!(InputValidator::validate_metadata_key("valid_key").is_ok());
        assert!(InputValidator::validate_metadata_key("valid-key").is_ok());
        assert!(InputValidator::validate_metadata_key("validKey123").is_ok());
        assert!(InputValidator::validate_metadata_key("").is_err());
        assert!(InputValidator::validate_metadata_key("invalid key").is_err());
        assert!(InputValidator::validate_metadata_key("invalid@key").is_err());
    }
}