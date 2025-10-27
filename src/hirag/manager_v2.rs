//! Enhanced HiRAG manager with improved concurrency and error handling

use super::{ContextManager, models::*, retriever::ContextRetriever, ranker::ContextRanker, token_estimator::TokenEstimator};
use crate::config::HiRAGConfig;
use crate::embedding::EmbeddingProvider;
use crate::error::{HiRAGError, Result};
use crate::vector_db::{ContextLevel, VectorPoint, VectorStore, Payload};
use crate::middleware::InputValidator;
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Enhanced HiRAG manager with improved concurrency safety
pub struct HiRAGManagerV2 {
    config: HiRAGConfig,
    embedding_client: Arc<dyn EmbeddingProvider>,
    vector_db: Arc<dyn VectorStore>,
    l1_cache: Arc<DashMap<Uuid, Context>>,
    l1_cache_size: Arc<AtomicUsize>,
    retriever: ContextRetriever,
    ranker: ContextRanker,
    token_estimator: TokenEstimator,
    metrics: Option<Arc<crate::observability::MetricsCollector>>,
}

impl HiRAGManagerV2 {
    /// Create a new HiRAG manager
    pub async fn new(
        config: HiRAGConfig,
        embedding_client: Arc<dyn EmbeddingProvider>,
        vector_db: Arc<dyn VectorStore>,
    ) -> Result<Self> {
        info!("Initializing enhanced HiRAG manager");
        
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
            l1_cache: Arc::new(DashMap::new()),
            l1_cache_size: Arc::new(AtomicUsize::new(0)),
            retriever,
            ranker,
            token_estimator,
            metrics: None,
        })
    }
    
    /// Set metrics collector
    pub fn with_metrics(mut self, metrics: Arc<crate::observability::MetricsCollector>) -> Self {
        self.metrics = Some(metrics);
        self
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
    
    /// Update L1 cache with lock-free DashMap
    async fn update_l1_cache(&self, context: Context) {
        let context_id = context.id;
        
        // Insert or update context (atomic operation)
        self.l1_cache.insert(context_id, context);
        
        // Update size counter
        let current_size = self.l1_cache.len();
        self.l1_cache_size.store(current_size, Ordering::Relaxed);
        
        // Maintain size limit by removing oldest entries
        if current_size > self.config.l1_size {
            // Collect IDs to remove (oldest entries)
            let mut entries: Vec<_> = self.l1_cache.iter()
                .map(|entry| (*entry.key(), entry.value().timestamp))
                .collect();
            
            // Sort by timestamp (oldest first)
            entries.sort_by_key(|(_, ts)| *ts);
            
            // Remove excess entries
            let to_remove = current_size - self.config.l1_size;
            for (id, _) in entries.iter().take(to_remove) {
                if let Some((_, removed)) = self.l1_cache.remove(id) {
                    debug!("Evicted context {} from L1 cache", removed.id);
                }
            }
            
            self.l1_cache_size.store(self.l1_cache.len(), Ordering::Relaxed);
        }
        
        debug!("L1 cache updated, size: {}", self.l1_cache.len());
    }
    
    /// Get contexts from L1 cache with lock-free access
    async fn get_l1_contexts(&self, max_tokens: usize) -> Vec<Context> {
        let mut contexts = Vec::new();
        let mut total_tokens = 0;
        
        // Collect all contexts and sort by timestamp (newest first)
        let mut all_contexts: Vec<_> = self.l1_cache.iter()
            .map(|entry| entry.value().clone())
            .collect();
        
        all_contexts.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        for context in all_contexts {
            if total_tokens + context.token_count <= max_tokens {
                total_tokens += context.token_count;
                contexts.push(context);
            } else {
                break;
            }
        }
        
        debug!("Retrieved {} contexts from L1 cache ({} tokens)", contexts.len(), total_tokens);
        contexts
    }
    
    /// Deduplicate contexts by ID
    fn deduplicate_contexts(&self, contexts: Vec<Context>) -> Vec<Context> {
        let mut seen_ids = HashSet::new();
        let mut deduplicated = Vec::new();
        let original_count = contexts.len();
        
        for context in contexts {
            if seen_ids.insert(context.id) {
                deduplicated.push(context);
            } else {
                debug!("Removed duplicate context: {}", context.id);
            }
        }
        
        debug!("Deduplicated {} -> {} contexts", original_count, deduplicated.len());
        deduplicated
    }
}

#[async_trait]
impl ContextManager for HiRAGManagerV2 {
    async fn store_context(
        &self,
        text: &str,
        level: ContextLevel,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Result<Uuid> {
        // Validate input
        InputValidator::validate_text(text)?;
        
        // Validate metadata keys
        for key in metadata.keys() {
            InputValidator::validate_metadata_key(key)?;
        }
        
        debug!("Storing context at level: {:?}", level);
        
        // Generate embedding
        let embedding = self.embedding_client.embed_single(text).await?;
        
        // Validate vector dimension
        InputValidator::validate_vector_dimension(
            embedding.len(),
            1024, // Expected dimension for multilingual-e5-large
        )?;
        
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
        
        // Record metrics
        if let Some(metrics) = &self.metrics {
            metrics.record_request(std::time::Instant::now().elapsed());
        }
        
        Ok(id)
    }
    
    async fn retrieve_context(&self, request: ContextRequest) -> Result<ContextResponse> {
        let start_time = std::time::Instant::now();
        
        // Validate input
        InputValidator::validate_text(&request.query)?;
        InputValidator::validate_token_count(request.max_tokens, 100000)?;
        
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
        
        // Retrieve from each level with partial failure handling
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
        
        // Wait for all parallel tasks with partial failure handling
        for task in tasks {
            match task.await {
                Ok(Ok(contexts)) => {
                    total_searched += contexts.len();
                    all_contexts.extend(contexts);
                }
                Ok(Err(e)) => {
                    warn!("Error retrieving contexts from one level: {}", e);
                    // Continue with other levels instead of failing completely
                }
                Err(e) => {
                    warn!("Task join error: {}", e);
                    // Continue with other levels
                }
            }
        }
        
        // Deduplicate contexts
        all_contexts = self.deduplicate_contexts(all_contexts);
        
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
        
        // Record metrics
        if let Some(metrics) = &self.metrics {
            metrics.record_request(start_time.elapsed());
            // Record cache hits
            for _ in 0..cache_hits {
                metrics.record_cache_hit();
            }
        }
        
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
        // Validate metadata keys
        for key in metadata.keys() {
            InputValidator::validate_metadata_key(key)?;
        }
        
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
                self.vector_db.insert_points(&collection, vec![point.clone()]).await?;
                
                // Update L1 cache if immediate level
                if *level == ContextLevel::Immediate {
                    let token_count = self.token_estimator.estimate(&point.payload.text);
                    let context = Context {
                        id: point.id,
                        text: point.payload.text,
                        level: point.payload.level,
                        relevance_score: 1.0,
                        token_count,
                        timestamp: point.payload.timestamp,
                        metadata: point.payload.metadata,
                    };
                    self.update_l1_cache(context).await;
                }
                
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
        
        // Remove from L1 cache (lock-free)
        self.l1_cache.remove(&id);
        self.l1_cache_size.store(self.l1_cache.len(), Ordering::Relaxed);
        
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
            self.l1_cache.clear();
            self.l1_cache_size.store(0, Ordering::Relaxed);
        }
        
        info!("Level cleared: {:?}", level);
        Ok(())
    }
}