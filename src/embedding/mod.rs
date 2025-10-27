//! Embedding service for generating vector embeddings via Chutes API

pub mod client;
pub mod client_v2;
pub mod cache;
pub mod models;

pub use client::EmbeddingClient;
pub use client_v2::EmbeddingClientV2;
pub use models::{EmbeddingRequest, EmbeddingResponse, EmbeddingInput};
pub use cache::EmbeddingCache;

use async_trait::async_trait;
use crate::error::Result;

/// Trait for embedding providers
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for a single text
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>>;
    
    /// Generate embeddings for multiple texts
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    
    /// Get the dimension of embeddings
    fn embedding_dimension(&self) -> usize;
}