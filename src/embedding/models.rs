//! Data models for embedding requests and responses

use serde::{Deserialize, Serialize};

/// Request to generate embeddings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    /// Input text(s) to embed
    pub input: EmbeddingInput,
    
    /// Model name (optional, defaults to configured model)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Input variants for embedding requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Batch(Vec<String>),
}

/// Response from embedding generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// Generated embeddings
    pub data: Vec<EmbeddingData>,
    
    /// Model used for generation
    pub model: String,
    
    /// Usage statistics
    pub usage: UsageStats,
}

/// Individual embedding data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingData {
    /// Embedding vector
    pub embedding: Vec<f32>,
    
    /// Index in the batch
    pub index: usize,
    
    /// Object type (always "embedding")
    pub object: String,
}

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    /// Number of prompt tokens
    pub prompt_tokens: usize,
    
    /// Total tokens used
    pub total_tokens: usize,
}

impl EmbeddingRequest {
    /// Create a request for a single text
    pub fn single(text: impl Into<String>) -> Self {
        Self {
            input: EmbeddingInput::Single(text.into()),
            model: None,
        }
    }
    
    /// Create a request for multiple texts
    pub fn batch(texts: Vec<String>) -> Self {
        Self {
            input: EmbeddingInput::Batch(texts),
            model: None,
        }
    }
}