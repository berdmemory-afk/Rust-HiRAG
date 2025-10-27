//! Data models for HiRAG operations

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use crate::vector_db::{ContextLevel, Filter};

/// Context item with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// Unique identifier
    pub id: Uuid,
    
    /// Text content
    pub text: String,
    
    /// Context level
    pub level: ContextLevel,
    
    /// Relevance score (0.0 - 1.0)
    pub relevance_score: f32,
    
    /// Estimated token count
    pub token_count: usize,
    
    /// Timestamp
    pub timestamp: i64,
    
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Request for context retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRequest {
    /// Query text
    pub query: String,
    
    /// Maximum tokens to retrieve
    pub max_tokens: usize,
    
    /// Specific levels to search (empty = all levels)
    #[serde(default)]
    pub levels: Vec<ContextLevel>,
    
    /// Metadata filters
    pub filters: Option<Filter>,
    
    /// Priority level
    #[serde(default)]
    pub priority: Priority,
    
    /// Session context
    pub session_id: Option<String>,
}

/// Priority levels for context retrieval
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

/// Response containing retrieved contexts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextResponse {
    /// Retrieved contexts (ordered by relevance)
    pub contexts: Vec<Context>,
    
    /// Total token count
    pub total_tokens: usize,
    
    /// Retrieval time in milliseconds
    pub retrieval_time_ms: u64,
    
    /// Metadata about retrieval
    pub metadata: ResponseMetadata,
}

/// Metadata about context retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMetadata {
    /// Number of contexts from each level
    pub level_distribution: HashMap<ContextLevel, usize>,
    
    /// Average relevance score
    pub avg_relevance: f32,
    
    /// Cache hit information
    pub cache_hits: usize,
    
    /// Total contexts searched
    pub total_searched: usize,
}

/// Statistics about HiRAG system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiRAGStats {
    /// Total contexts stored
    pub total_contexts: usize,
    
    /// Contexts per level
    pub contexts_per_level: HashMap<ContextLevel, usize>,
    
    /// Average retrieval time (ms)
    pub avg_retrieval_time_ms: f64,
    
    /// Cache hit rate
    pub cache_hit_rate: f64,
    
    /// Average relevance score
    pub avg_relevance_score: f32,
    
    /// Total storage size (bytes)
    pub storage_size_bytes: usize,
}

impl Context {
    pub fn new(
        id: Uuid,
        text: String,
        level: ContextLevel,
        timestamp: i64,
        token_count: usize,
    ) -> Self {
        Self {
            id,
            text,
            level,
            relevance_score: 0.0,
            token_count,
            timestamp,
            metadata: HashMap::new(),
        }
    }
}

impl ContextRequest {
    pub fn new(query: String, max_tokens: usize) -> Self {
        Self {
            query,
            max_tokens,
            levels: Vec::new(),
            filters: None,
            priority: Priority::Normal,
            session_id: None,
        }
    }
    
    pub fn with_levels(mut self, levels: Vec<ContextLevel>) -> Self {
        self.levels = levels;
        self
    }
    
    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }
}
/// Search query for API endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Query text
    pub query: String,
    
    /// Maximum number of results
    pub limit: Option<usize>,
    
    /// Optional filter
    pub filter: Option<ContextFilter>,
}

/// Filter for context search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFilter {
    /// Filter by context level
    pub level: Option<ContextLevel>,
    
    /// Filter by tags
    pub tags: Option<Vec<String>>,
    
    /// Filter by expiration date (before)
    pub expires_before: Option<chrono::DateTime<chrono::Utc>>,
    
    /// Filter by creation date (after)
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
}
