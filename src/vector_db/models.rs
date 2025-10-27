//! Data models for vector database operations

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Context hierarchy levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ContextLevel {
    Immediate,  // L1
    ShortTerm,  // L2
    LongTerm,   // L3
}

impl ContextLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContextLevel::Immediate => "Immediate",
            ContextLevel::ShortTerm => "ShortTerm",
            ContextLevel::LongTerm => "LongTerm",
        }
    }
}

/// Point to be stored in vector database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorPoint {
    /// Unique identifier
    pub id: Uuid,
    
    /// Vector embedding
    pub vector: Vec<f32>,
    
    /// Associated metadata
    pub payload: Payload,
}

/// Metadata payload for vector points
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    /// Original text content
    pub text: String,
    
    /// Context level (L1, L2, L3)
    pub level: ContextLevel,
    
    /// Timestamp of creation
    pub timestamp: i64,
    
    /// Agent or user identifier
    pub agent_id: String,
    
    /// Session identifier
    pub session_id: Option<String>,
    
    /// Additional metadata
    #[serde(flatten)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Search parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchParams {
    /// Query vector
    pub vector: Vec<f32>,
    
    /// Maximum number of results
    pub limit: usize,
    
    /// Minimum similarity score
    pub score_threshold: Option<f32>,
    
    /// Metadata filters
    pub filter: Option<Filter>,
    
    /// Include payload in results
    pub with_payload: bool,
    
    /// Include vectors in results
    pub with_vector: bool,
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Point ID
    pub id: Uuid,
    
    /// Similarity score
    pub score: f32,
    
    /// Payload (if requested)
    pub payload: Option<Payload>,
    
    /// Vector (if requested)
    pub vector: Option<Vec<f32>>,
}

/// Filter for metadata-based search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    /// Must match all conditions
    #[serde(default)]
    pub must: Vec<Condition>,
    
    /// Must match at least one condition
    #[serde(default)]
    pub should: Vec<Condition>,
    
    /// Must not match any condition
    #[serde(default)]
    pub must_not: Vec<Condition>,
}

/// Individual filter condition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Condition {
    Match { key: String, value: serde_json::Value },
    Range { key: String, gte: Option<f64>, lte: Option<f64> },
    HasId { ids: Vec<Uuid> },
}

impl SearchParams {
    pub fn new(vector: Vec<f32>, limit: usize) -> Self {
        Self {
            vector,
            limit,
            score_threshold: None,
            filter: None,
            with_payload: true,
            with_vector: false,
        }
    }
    
    pub fn with_score_threshold(mut self, threshold: f32) -> Self {
        self.score_threshold = Some(threshold);
        self
    }
    
    pub fn with_filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }
}

impl Filter {
    pub fn new() -> Self {
        Self {
            must: Vec::new(),
            should: Vec::new(),
            must_not: Vec::new(),
        }
    }
    
    pub fn must(mut self, condition: Condition) -> Self {
        self.must.push(condition);
        self
    }
    
    pub fn should(mut self, condition: Condition) -> Self {
        self.should.push(condition);
        self
    }
    
    pub fn must_not(mut self, condition: Condition) -> Self {
        self.must_not.push(condition);
        self
    }
}

impl Default for Filter {
    fn default() -> Self {
        Self::new()
    }
}