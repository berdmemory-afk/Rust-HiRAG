//! Vector database integration with Qdrant

pub mod client;
pub mod client_v2;
pub mod models;
pub mod search;
pub mod circuit_breaker;

pub use client::VectorDbClient;
pub use models::{VectorPoint, Payload, SearchParams, SearchResult, Filter, Condition, ContextLevel};
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};

use async_trait::async_trait;
use crate::error::Result;
use uuid::Uuid;

/// Trait for vector storage operations
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Create a new collection
    async fn create_collection(&self, name: &str) -> Result<()>;
    
    /// Delete a collection
    async fn delete_collection(&self, name: &str) -> Result<()>;
    
    /// Insert points into collection
    async fn insert_points(&self, collection: &str, points: Vec<VectorPoint>) -> Result<()>;
    
    /// Search for similar vectors
    async fn search(&self, collection: &str, params: SearchParams) -> Result<Vec<SearchResult>>;
    
    /// Delete points by ID
    async fn delete_points(&self, collection: &str, ids: Vec<Uuid>) -> Result<()>;
    
    /// Get point by ID
    async fn get_point(&self, collection: &str, id: Uuid) -> Result<Option<VectorPoint>>;
}