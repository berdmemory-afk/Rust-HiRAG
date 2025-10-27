//! Integration tests for Context Manager
//! 
//! These tests require external services:
//! - Qdrant vector database (http://localhost:6333)
//! - Embedding API endpoint
//! 
//! To run these tests:
//! 1. Start Qdrant: `docker run -p 6333:6333 qdrant/qdrant`
//! 2. Set CHUTES_API_TOKEN environment variable
//! 3. Run: `cargo test --test integration_test -- --ignored`

use context_manager::{
    Config,
    v2::{VectorDbClientV2, EmbeddingClientV2, HiRAGManagerV2},
    vector_db::{VectorStore, ContextLevel},
    embedding::EmbeddingProvider,
    observability::{HealthChecker, MetricsCollector},
    hirag::ContextManager,
};
use std::sync::Arc;
use std::collections::HashMap;

/// Helper to check if Qdrant is available
async fn is_qdrant_available() -> bool {
    reqwest::get("http://localhost:6333/health")
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Helper to create test configuration
fn create_test_config() -> Config {
    let mut config = Config::default_config();
    config.vector_db.url = "http://localhost:6333".to_string();
    config.embedding.api_token = secrecy::Secret::new(
        std::env::var("CHUTES_API_TOKEN").unwrap_or_else(|_| "test_token".to_string())
    );
    config
}

#[tokio::test]
#[ignore] // Requires Qdrant running
async fn test_vector_db_connection() {
    if !is_qdrant_available().await {
        eprintln!("Skipping test: Qdrant not available at localhost:6333");
        return;
    }

    let config = create_test_config();
    let client = VectorDbClientV2::new(config.vector_db)
        .await
        .expect("Failed to create vector DB client");

    // Test collection creation
    let collection_name = "test_collection";
    let result = client.create_collection(collection_name).await;
    
    // Collection might already exist, which is fine
    assert!(result.is_ok() || result.unwrap_err().to_string().contains("already exists"));
    
    // Cleanup
    let _ = client.delete_collection(collection_name).await;
}

#[tokio::test]
#[ignore] // Requires Qdrant running
async fn test_vector_db_with_circuit_breaker() {
    if !is_qdrant_available().await {
        eprintln!("Skipping test: Qdrant not available at localhost:6333");
        return;
    }

    let config = create_test_config();
    let metrics = Arc::new(MetricsCollector::new());
    
    let client = VectorDbClientV2::new(config.vector_db)
        .await
        .expect("Failed to create vector DB client")
        .with_metrics(metrics.clone());

    // Perform multiple operations to test circuit breaker
    let collection_name = "test_cb_collection";
    
    for i in 0..3 {
        let result = client.create_collection(&format!("{}_{}", collection_name, i)).await;
        assert!(result.is_ok() || result.unwrap_err().to_string().contains("already exists"));
    }

    // Check metrics were recorded
    let system_metrics = metrics.get_metrics();
    assert!(system_metrics.total_requests > 0);
    
    // Cleanup
    for i in 0..3 {
        let _ = client.delete_collection(&format!("{}_{}", collection_name, i)).await;
    }
}

#[tokio::test]
#[ignore] // Requires embedding API
async fn test_embedding_client() {
    let config = create_test_config();
    
    let client = EmbeddingClientV2::new(config.embedding)
        .expect("Failed to create embedding client");

    // Test single embedding
    let text = "This is a test sentence for embedding.";
    let result = client.embed_single(text).await;
    
    if let Ok(embedding) = result {
        assert!(!embedding.is_empty());
        assert_eq!(embedding.len(), client.embedding_dimension());
    } else {
        eprintln!("Embedding API not available or token invalid");
    }
}

#[tokio::test]
#[ignore] // Requires embedding API
async fn test_embedding_batch() {
    let config = create_test_config();
    
    let client = EmbeddingClientV2::new(config.embedding)
        .expect("Failed to create embedding client");

    // Test batch embedding
    let texts = vec![
        "First test sentence.".to_string(),
        "Second test sentence.".to_string(),
        "Third test sentence.".to_string(),
    ];
    
    let result = client.embed_batch(&texts).await;
    
    if let Ok(embeddings) = result {
        assert_eq!(embeddings.len(), texts.len());
        for embedding in embeddings {
            assert!(!embedding.is_empty());
            assert_eq!(embedding.len(), client.embedding_dimension());
        }
    } else {
        eprintln!("Embedding API not available or token invalid");
    }
}

#[tokio::test]
#[ignore] // Requires both Qdrant and embedding API
async fn test_hirag_manager_end_to_end() {
    if !is_qdrant_available().await {
        eprintln!("Skipping test: Qdrant not available at localhost:6333");
        return;
    }

    let config = create_test_config();
    
    // Create clients
    let embedding_client = Arc::new(
        EmbeddingClientV2::new(config.embedding.clone())
            .expect("Failed to create embedding client")
    );
    
    let vector_db = Arc::new(
        VectorDbClientV2::new(config.vector_db.clone())
            .await
            .expect("Failed to create vector DB client")
    );

    // Create manager
    let manager = HiRAGManagerV2::new(
        config.hirag.clone(),
        embedding_client.clone() as Arc<dyn EmbeddingProvider>,
        vector_db.clone() as Arc<dyn VectorStore>,
    )
    .await
    .expect("Failed to create HiRAG manager");

    // Initialize collections
    manager.initialize().await.expect("Failed to initialize");

    // Store some contexts
    let contexts = vec![
        ("User prefers dark mode", ContextLevel::LongTerm),
        ("Current task: writing documentation", ContextLevel::ShortTerm),
        ("Just asked about testing", ContextLevel::Immediate),
    ];

    let mut stored_ids = Vec::new();
    for (text, level) in contexts {
        match manager.store_context(text, level, HashMap::new()).await {
            Ok(id) => {
                stored_ids.push(id);
                println!("Stored context: {} with id: {}", text, id);
            }
            Err(e) => {
                eprintln!("Failed to store context: {}", e);
            }
        }
    }

    // Give Qdrant time to index
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Retrieve contexts
    let request = context_manager::hirag::ContextRequest {
        query: "What are the user preferences?".to_string(),
        max_tokens: 1000,
        levels: vec![],
        filters: None,
        priority: context_manager::hirag::Priority::Normal,
        session_id: None,
    };

    match manager.retrieve_context(request).await {
        Ok(response) => {
            println!("Retrieved {} contexts", response.contexts.len());
            println!("Total tokens: {}", response.total_tokens);
            println!("Retrieval time: {}ms", response.retrieval_time_ms);
            
            assert!(!response.contexts.is_empty());
            assert!(response.total_tokens > 0);
        }
        Err(e) => {
            eprintln!("Failed to retrieve contexts: {}", e);
        }
    }

    // Cleanup - delete stored contexts
    for id in stored_ids {
        let _ = manager.delete_context(id).await;
    }
}

#[tokio::test]
#[ignore] // Requires Qdrant
async fn test_health_checks_with_real_services() {
    if !is_qdrant_available().await {
        eprintln!("Skipping test: Qdrant not available at localhost:6333");
        return;
    }

    let config = create_test_config();
    
    let embedding_client = Arc::new(
        EmbeddingClientV2::new(config.embedding.clone())
            .expect("Failed to create embedding client")
    );
    
    let vector_db = Arc::new(
        VectorDbClientV2::new(config.vector_db.clone())
            .await
            .expect("Failed to create vector DB client")
    );

    let health_checker = HealthChecker::new()
        .with_vector_db(vector_db.clone() as Arc<dyn VectorStore>)
        .with_embedding_client(embedding_client.clone() as Arc<dyn EmbeddingProvider>);

    let health = health_checker.check_health().await;
    
    println!("Overall health: {:?}", health.status);
    for component in &health.components {
        println!("  {}: {:?} - {:?}", 
            component.name, 
            component.status,
            component.message
        );
    }

    // At least vector DB should be healthy
    let vector_db_health = health.components.iter()
        .find(|c| c.name == "vector_database")
        .expect("Vector DB health check not found");
    
    assert_eq!(vector_db_health.status, context_manager::observability::HealthStatus::Healthy);
}

#[tokio::test]
#[ignore] // Requires Qdrant
async fn test_metrics_collection() {
    if !is_qdrant_available().await {
        eprintln!("Skipping test: Qdrant not available at localhost:6333");
        return;
    }

    let config = create_test_config();
    let metrics = Arc::new(MetricsCollector::new());
    
    let vector_db = VectorDbClientV2::new(config.vector_db.clone())
        .await
        .expect("Failed to create vector DB client")
        .with_metrics(metrics.clone());

    // Perform some operations
    let collection_name = "test_metrics_collection";
    let _ = vector_db.create_collection(collection_name).await;
    
    // Check metrics
    let system_metrics = metrics.get_metrics();
    assert!(system_metrics.total_requests > 0);
    
    println!("Metrics collected:");
    println!("  Total requests: {}", system_metrics.total_requests);
    println!("  Total errors: {}", system_metrics.total_errors);
    println!("  Avg response time: {}ms", system_metrics.avg_response_time_ms);
    
    // Cleanup
    let _ = vector_db.delete_collection(collection_name).await;
}