//! HiRAG manager implementation

use super::{ContextManager, models::*, retriever::ContextRetriever, ranker::ContextRanker, token_estimator::TokenEstimator};
use crate::config::HiRAGConfig;
use crate::embedding::EmbeddingProvider;
use crate::error::{HiRAGError, Result};
use crate::vector_db::{ContextLevel, VectorPoint, VectorStore, Payload};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};
use uuid::Uuid;

/// HiRAG manager for hierarchical context management
pub struct HiRAGManager {
    config: HiRAGConfig,
    embedding_client: Arc<dyn EmbeddingProvider>,
    vector_db: Arc<dyn VectorStore>,
    l1_cache: Arc<RwLock<VecDeque<Context>>>,
    retriever: ContextRetriever,
    ranker: ContextRanker,
    token_estimator: TokenEstimator,
}

impl HiRAGManager {
    /// Create a new HiRAG manager
    pub async fn new(
        config: HiRAGConfig,
        embedding_client: Arc<dyn EmbeddingProvider>,
        vector_db: Arc<dyn VectorStore>,
    ) -> Result<Self> {
        info!("Initializing HiRAG manager");
        
        let token_estimator = TokenEstimator::new(config.token_estimator);
        let retriever = ContextRetriever::new(
            vector_db.clone(),
            TokenEstimator::new(config.token_estimator),
            config.retrieval_strategy.clone(),
        );
        let ranker = ContextRanker::new(config.ranking_weights.clone());
        
        Ok(Self {
            config,
            embedding_client,
            vector_db,
            l1_cache: Arc::new(RwLock::new(VecDeque::new())),
            retriever,
            ranker,
            token_estimator,
        })
    }
    
    /// Initialize the manager
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing HiRAG collections");
        
        // Create collections for each level
        for level in &[ContextLevel::Immediate, ContextLevel::ShortTerm, ContextLevel::LongTerm] {
            let collection_name = self.collection_name(*level);
            
            // Try to create collection (will fail if exists, which is fine)
            let _ = self.vector_db.create_collection(&collection_name).await;
        }
        
        Ok(())
    }
    
    /// Get collection name for a context level
    fn collection_name(&self, level: ContextLevel) -> String {
        format!("contexts_{}", level.as_str().to_lowercase())
    }
    
    /// Update L1 cache
    async fn update_l1_cache(&self, context: Context) {
        let mut cache = self.l1_cache.write().await;
        
        // Add to front
        cache.push_front(context);
        
        // Maintain size limit
        while cache.len() > self.config.l1_size {
            cache.pop_back();
        }
        
        debug!("L1 cache updated, size: {}", cache.len());
    }
    
    /// Get contexts from L1 cache
    async fn get_l1_contexts(&self, max_tokens: usize) -> Vec<Context> {
        let cache = self.l1_cache.read().await;
        let mut contexts = Vec::new();
        let mut total_tokens = 0;
        
        for context in cache.iter() {
            if total_tokens + context.token_count <= max_tokens {
                contexts.push(context.clone());
                total_tokens += context.token_count;
            } else {
                break;
            }
        }
        
        debug!("Retrieved {} contexts from L1 cache", contexts.len());
        contexts
    }
}

#[async_trait]
impl ContextManager for HiRAGManager {
    async fn store_context(
        &self,
        text: &str,
        level: ContextLevel,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Result<Uuid> {
        debug!("Storing context at level: {:?}", level);
        
        // Generate embedding
        let embedding = self.embedding_client.embed_single(text).await?;
        
        // Create point
        let id = Uuid::new_v4();
        let timestamp = Utc::now().timestamp();
        let token_count = self.token_estimator.estimate(text);
        
        let point = VectorPoint {
            id,
            vector: embedding,
            payload: Payload {
                text: text.to_string(),
                level,
                timestamp,
                agent_id: "default".to_string(),
                session_id: None,
                metadata: metadata.clone(),
            },
        };
        
        // Store in vector database
        let collection = self.collection_name(level);
        self.vector_db.insert_points(&collection, vec![point]).await?;
        
        // Update L1 cache if immediate context
        if level == ContextLevel::Immediate {
            let context = Context {
                id,
                text: text.to_string(),
                level,
                relevance_score: 1.0,
                token_count,
                timestamp,
                metadata,
            };
            self.update_l1_cache(context).await;
        }
        
        info!("Context stored with id: {}", id);
        Ok(id)
    }
    
    async fn retrieve_context(&self, request: ContextRequest) -> Result<ContextResponse> {
        let start_time = std::time::Instant::now();
        debug!("Retrieving context for query: {}", request.query);
        
        // Generate query embedding
        let query_embedding = self.embedding_client.embed_single(&request.query).await?;
        
        // Determine which levels to search
        let levels = if request.levels.is_empty() {
            vec![ContextLevel::Immediate, ContextLevel::ShortTerm, ContextLevel::LongTerm]
        } else {
            request.levels.clone()
        };
        
        // Calculate token allocations
        let (l1_tokens, l2_tokens, l3_tokens) = self.retriever.calculate_allocations(request.max_tokens);
        
        let mut all_contexts = Vec::new();
        let mut cache_hits = 0;
        let mut total_searched = 0;
        
        // Retrieve from each level in parallel
        let mut tasks = Vec::new();
        
        for level in levels {
            let max_tokens = match level {
                ContextLevel::Immediate => l1_tokens,
                ContextLevel::ShortTerm => l2_tokens,
                ContextLevel::LongTerm => l3_tokens,
            };
            
            if level == ContextLevel::Immediate {
                // Use L1 cache (synchronous)
                cache_hits += 1;
                let contexts = self.get_l1_contexts(max_tokens).await;
                total_searched += contexts.len();
                all_contexts.extend(contexts);
            } else {
                // Search vector database in parallel
                let collection = self.collection_name(level);
                let retriever = self.retriever.clone();
                let embedding = query_embedding.clone();
                let filters = request.filters.clone();
                
                tasks.push(tokio::spawn(async move {
                    retriever.retrieve_from_level(
                        &collection,
                        embedding,
                        max_tokens,
                        filters,
                    ).await
                }));
            }
        }
        
        // Wait for all parallel tasks to complete
        for task in tasks {
            match task.await {
                Ok(Ok(contexts)) => {
                    total_searched += contexts.len();
                    all_contexts.extend(contexts);
                }
                Ok(Err(e)) => {
                    error!("Error retrieving contexts: {}", e);
                    return Err(e);
                }
                Err(e) => {
                    error!("Task join error: {}", e);
                    return Err(HiRAGError::RetrievalError(e.to_string()).into());
                }
            }
        }
        
        // Rank contexts
        let ranked_contexts = self.ranker.rank_contexts(all_contexts);
        
        // Apply token limit
        let mut final_contexts = Vec::new();
        let mut total_tokens = 0;
        
        for context in ranked_contexts {
            if total_tokens + context.token_count <= request.max_tokens {
                total_tokens += context.token_count;
                final_contexts.push(context);
            }
        }
        
        // Calculate metadata
        let mut level_distribution = HashMap::new();
        for context in &final_contexts {
            *level_distribution.entry(context.level).or_insert(0) += 1;
        }
        
        let avg_relevance = if !final_contexts.is_empty() {
            final_contexts.iter().map(|c| c.relevance_score).sum::<f32>() / final_contexts.len() as f32
        } else {
            0.0
        };
        
        let retrieval_time_ms = start_time.elapsed().as_millis() as u64;
        
        info!(
            "Retrieved {} contexts in {}ms (total tokens: {})",
            final_contexts.len(),
            retrieval_time_ms,
            total_tokens
        );
        
        Ok(ContextResponse {
            contexts: final_contexts,
            total_tokens,
            retrieval_time_ms,
            metadata: ResponseMetadata {
                level_distribution,
                avg_relevance,
                cache_hits,
                total_searched,
            },
        })
    }
    
    async fn update_context(
        &self,
        id: Uuid,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        debug!("Updating context: {}", id);
        
        // Try to find and update the context in all collections
        for level in &[ContextLevel::Immediate, ContextLevel::ShortTerm, ContextLevel::LongTerm] {
            let collection = self.collection_name(*level);
            
            // Try to get the existing point
            if let Some(mut point) = self.vector_db.get_point(&collection, id).await? {
                // Update metadata
                for (key, value) in metadata.iter() {
                    point.payload.metadata.insert(key.clone(), value.clone());
                }
                
                // Update timestamp
                point.payload.timestamp = chrono::Utc::now().timestamp();
                
                // Re-insert the updated point
                self.vector_db.insert_points(&collection, vec![point]).await?;
                
                info!("Updated context {} in collection {}", id, collection);
                return Ok(());
            }
        }
        
        Err(HiRAGError::StorageError(format!("Context {} not found", id)).into())
    }
    
    async fn delete_context(&self, id: Uuid) -> Result<()> {
        debug!("Deleting context: {}", id);
        
        // Try to delete from all collections
        for level in &[ContextLevel::Immediate, ContextLevel::ShortTerm, ContextLevel::LongTerm] {
            let collection = self.collection_name(*level);
            let _ = self.vector_db.delete_points(&collection, vec![id]).await;
        }
        
        // Remove from L1 cache
        let mut cache = self.l1_cache.write().await;
        cache.retain(|c| c.id != id);
        
        info!("Context deleted: {}", id);
        Ok(())
    }
    
    async fn clear_level(&self, level: ContextLevel) -> Result<()> {
        debug!("Clearing level: {:?}", level);
        
        let collection = self.collection_name(level);
        
        // Delete and recreate collection
        let _ = self.vector_db.delete_collection(&collection).await;
        self.vector_db.create_collection(&collection).await?;
        
        // Clear L1 cache if immediate level
        if level == ContextLevel::Immediate {
            let mut cache = self.l1_cache.write().await;
            cache.clear();
        }
        
        info!("Level cleared: {:?}", level);
        Ok(())
    }
}