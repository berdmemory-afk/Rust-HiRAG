//! Context ranking and scoring

use super::models::Context;
use crate::config::RankingWeights;
use chrono::Utc;

/// Context ranker for scoring and ordering
pub struct ContextRanker {
    weights: RankingWeights,
}

impl ContextRanker {
    pub fn new(weights: RankingWeights) -> Self {
        Self { weights }
    }
    
    /// Rank contexts based on multiple factors
    pub fn rank_contexts(&self, mut contexts: Vec<Context>) -> Vec<Context> {
        let current_time = Utc::now().timestamp();
        
        for context in &mut contexts {
            context.relevance_score = self.calculate_score(context, current_time);
        }
        
        // Sort by relevance score (descending)
        contexts.sort_by(|a, b| {
            b.relevance_score.partial_cmp(&a.relevance_score).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        contexts
    }
    
    /// Calculate composite score for a context
    pub fn calculate_score(&self, context: &Context, current_time: i64) -> f32 {
        let similarity_score = context.relevance_score; // Already set from vector search
        let recency_score = self.calculate_recency_score(context.timestamp, current_time);
        let level_score = self.calculate_level_score(context.level);
        let frequency_score = self.calculate_frequency_score(context);
        
        similarity_score * self.weights.similarity_weight
            + recency_score * self.weights.recency_weight
            + level_score * self.weights.level_weight
            + frequency_score * self.weights.frequency_weight
    }
    
    /// Calculate recency score (more recent = higher score)
    fn calculate_recency_score(&self, timestamp: i64, current_time: i64) -> f32 {
        let age_seconds = (current_time - timestamp).max(0) as f32;
        let age_hours = age_seconds / 3600.0;
        
        // Exponential decay: score = e^(-age_hours / 24)
        // After 24 hours, score is ~0.37
        // After 48 hours, score is ~0.14
        (-age_hours / 24.0).exp()
    }
    
    /// Calculate level score (L1 > L2 > L3)
    fn calculate_level_score(&self, level: crate::vector_db::ContextLevel) -> f32 {
        use crate::vector_db::ContextLevel;
        
        match level {
            ContextLevel::Immediate => 1.0,
            ContextLevel::ShortTerm => 0.7,
            ContextLevel::LongTerm => 0.5,
        }
    }
    
    /// Calculate frequency score based on access count in metadata
    fn calculate_frequency_score(&self, context: &Context) -> f32 {
        let access_count = context.metadata
            .get("access_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as f32;
        
        // Logarithmic scaling: score = log(1 + access_count) / log(101)
        // Max score of 1.0 at 100 accesses
        if access_count > 0.0 {
            (1.0 + access_count).log10() / 101_f32.log10()
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector_db::ContextLevel;
    
    
    #[test]
    fn test_recency_score() {
        let weights = RankingWeights {
            similarity_weight: 0.5,
            recency_weight: 0.2,
            level_weight: 0.2,
            frequency_weight: 0.1,
        };
        let ranker = ContextRanker::new(weights);
        
        let current_time = Utc::now().timestamp();
        let one_hour_ago = current_time - 3600;
        
        let score = ranker.calculate_recency_score(one_hour_ago, current_time);
        assert!(score > 0.9 && score <= 1.0);
    }
    
    #[test]
    fn test_level_score() {
        let weights = RankingWeights {
            similarity_weight: 0.5,
            recency_weight: 0.2,
            level_weight: 0.2,
            frequency_weight: 0.1,
        };
        let ranker = ContextRanker::new(weights);
        
        assert_eq!(ranker.calculate_level_score(ContextLevel::Immediate), 1.0);
        assert_eq!(ranker.calculate_level_score(ContextLevel::ShortTerm), 0.7);
        assert_eq!(ranker.calculate_level_score(ContextLevel::LongTerm), 0.5);
    }
}