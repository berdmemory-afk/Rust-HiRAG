//! Context retrieval logic for different levels

use super::models::*;
use super::token_estimator::TokenEstimator;
use crate::config::RetrievalStrategy;
use crate::error::Result;
use crate::vector_db::{SearchParams, VectorStore};
use std::sync::Arc;
use tracing::debug;

/// Context retriever for hierarchical retrieval
#[derive(Clone)]
pub struct ContextRetriever {
    vector_db: Arc<dyn VectorStore>,
    token_estimator: TokenEstimator,
    strategy: RetrievalStrategy,
}

impl ContextRetriever {
    pub fn new(
        vector_db: Arc<dyn VectorStore>,
        token_estimator: TokenEstimator,
        strategy: RetrievalStrategy,
    ) -> Self {
        Self {
            vector_db,
            token_estimator,
            strategy,
        }
    }
    
    /// Retrieve contexts from a specific level
    pub async fn retrieve_from_level(
        &self,
        collection: &str,
        query_vector: Vec<f32>,
        max_tokens: usize,
        filters: Option<crate::vector_db::Filter>,
    ) -> Result<Vec<Context>> {
        debug!("Retrieving from level: {} with max_tokens: {}", collection, max_tokens);
        
        // Search with generous limit, we'll filter by tokens later
        let search_params = SearchParams {
            vector: query_vector,
            limit: 100,
            score_threshold: None,
            filter: filters,
            with_payload: true,
            with_vector: false,
        };
        
        let results = self.vector_db.search(collection, search_params).await?;
        
        // Convert to Context objects and filter by token budget
        let mut contexts = Vec::new();
        let mut total_tokens = 0;
        
        for result in results {
            if let Some(payload) = result.payload {
                let token_count = self.token_estimator.estimate(&payload.text);
                
                if total_tokens + token_count <= max_tokens {
                    contexts.push(Context {
                        id: result.id,
                        text: payload.text,
                        level: payload.level,
                        relevance_score: result.score,
                        token_count,
                        timestamp: payload.timestamp,
                        metadata: payload.metadata,
                    });
                    
                    total_tokens += token_count;
                } else {
                    break;
                }
            }
        }
        
        debug!("Retrieved {} contexts with {} tokens", contexts.len(), total_tokens);
        Ok(contexts)
    }
    
    /// Calculate token allocation for each level
    pub fn calculate_allocations(&self, max_tokens: usize) -> (usize, usize, usize) {
        let l1_tokens = (max_tokens as f32 * self.strategy.l1_allocation) as usize;
        let l2_tokens = (max_tokens as f32 * self.strategy.l2_allocation) as usize;
        let l3_tokens = (max_tokens as f32 * self.strategy.l3_allocation) as usize;
        
        (l1_tokens, l2_tokens, l3_tokens)
    }
    
    /// Calculate dynamic allocations based on available contexts
    pub fn calculate_dynamic_allocations(
        &self,
        max_tokens: usize,
        l1_available: usize,
        l2_available: usize,
        l3_available: usize,
    ) -> (usize, usize, usize) {
        let (mut l1_tokens, mut l2_tokens, mut l3_tokens) = self.calculate_allocations(max_tokens);
        
        // Redistribute unused tokens from levels with no contexts
        let mut unused_tokens = 0;
        let mut active_levels = 0;
        
        if l1_available == 0 {
            unused_tokens += l1_tokens;
            l1_tokens = 0;
        } else {
            active_levels += 1;
        }
        
        if l2_available == 0 {
            unused_tokens += l2_tokens;
            l2_tokens = 0;
        } else {
            active_levels += 1;
        }
        
        if l3_available == 0 {
            unused_tokens += l3_tokens;
            l3_tokens = 0;
        } else {
            active_levels += 1;
        }
        
        // Redistribute unused tokens equally among active levels
        if active_levels > 0 && unused_tokens > 0 {
            let redistribution = unused_tokens / active_levels;
            
            if l1_available > 0 {
                l1_tokens += redistribution;
            }
            if l2_available > 0 {
                l2_tokens += redistribution;
            }
            if l3_available > 0 {
                l3_tokens += redistribution;
            }
        }
        
        (l1_tokens, l2_tokens, l3_tokens)
    }
}