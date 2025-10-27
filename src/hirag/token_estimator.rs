//! Token estimation utilities

use crate::config::TokenEstimator as TokenEstimatorConfig;

/// Token estimator for calculating token counts
#[derive(Clone)]
pub struct TokenEstimator {
    config: TokenEstimatorConfig,
}

impl TokenEstimator {
    pub fn new(config: TokenEstimatorConfig) -> Self {
        Self { config }
    }
    
    /// Estimate token count for text
    pub fn estimate(&self, text: &str) -> usize {
        match &self.config {
            TokenEstimatorConfig::CharacterBased { chars_per_token } => {
                let char_count = text.chars().count();
                (char_count as f32 / chars_per_token).ceil() as usize
            }
            TokenEstimatorConfig::WordBased { words_per_token } => {
                let word_count = text.split_whitespace().count();
                (word_count as f32 / words_per_token).ceil() as usize
            }
        }
    }
    
    /// Estimate total tokens for multiple texts
    pub fn estimate_batch(&self, texts: &[String]) -> Vec<usize> {
        texts.iter().map(|text| self.estimate(text)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_character_based_estimation() {
        let config = TokenEstimatorConfig::CharacterBased { chars_per_token: 4.0 };
        let estimator = TokenEstimator::new(config);
        
        let text = "Hello world";
        let tokens = estimator.estimate(text);
        
        assert_eq!(tokens, 3); // 11 chars / 4 = 2.75 -> 3
    }
    
    #[test]
    fn test_word_based_estimation() {
        let config = TokenEstimatorConfig::WordBased { words_per_token: 1.3 };
        let estimator = TokenEstimator::new(config);
        
        let text = "Hello world test";
        let tokens = estimator.estimate(text);
        
        assert_eq!(tokens, 3); // 3 words / 1.3 = 2.3 -> 3
    }
}