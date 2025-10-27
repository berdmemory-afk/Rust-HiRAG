//! Basic usage example for the context manager

use context_manager::prelude::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize observability
    context_manager::observability::init_observability("info", "pretty");
    
    println!("=== Context Manager Basic Usage Example ===\n");
    
    // Load configuration
    let mut config = Config::default_config();
    
    // Set API token from environment
    config.embedding.api_token = secrecy::Secret::new(
        std::env::var("CHUTES_API_TOKEN")
            .expect("CHUTES_API_TOKEN environment variable not set")
    );
    
    println!("1. Initializing components...");
    
    // Initialize embedding client
    let embedding_client = std::sync::Arc::new(
        EmbeddingClient::new(config.embedding.clone())?
    );
    println!("   ✓ Embedding client initialized");
    
    // Initialize vector database
    let vector_db = std::sync::Arc::new(
        VectorDbClient::new(config.vector_db.clone()).await?
    );
    println!("   ✓ Vector database client initialized");
    
    // Initialize collections
    vector_db.initialize_collections().await?;
    println!("   ✓ Collections initialized");
    
    // Initialize HiRAG manager
    let hirag_manager = std::sync::Arc::new(
        HiRAGManager::new(
            config.hirag.clone(),
            embedding_client.clone(),
            vector_db.clone(),
        ).await?
    );
    
    hirag_manager.initialize().await?;
    println!("   ✓ HiRAG manager initialized\n");
    
    // Store some contexts
    println!("2. Storing contexts...");
    
    let contexts = vec![
        ("The weather is sunny today", ContextLevel::Immediate),
        ("User prefers outdoor activities", ContextLevel::LongTerm),
        ("Last conversation was about travel plans", ContextLevel::ShortTerm),
        ("User mentioned liking Italian food", ContextLevel::LongTerm),
        ("Current topic is vacation planning", ContextLevel::Immediate),
    ];
    
    for (text, level) in contexts {
        let id = hirag_manager.store_context(
            text,
            level,
            HashMap::new(),
        ).await?;
        println!("   ✓ Stored: {} (level: {:?}, id: {})", text, level, id);
    }
    
    println!("\n3. Retrieving relevant contexts...");
    
    // Create a context request
    let request = ContextRequest::new(
        "What does the user like to do?".to_string(),
        2000, // max tokens
    );
    
    // Retrieve contexts
    let response = hirag_manager.retrieve_context(request).await?;
    
    println!("   Retrieved {} contexts in {}ms", 
        response.contexts.len(), 
        response.retrieval_time_ms
    );
    println!("   Total tokens: {}", response.total_tokens);
    println!("   Average relevance: {:.2}", response.metadata.avg_relevance);
    
    println!("\n4. Retrieved contexts:");
    for (i, context) in response.contexts.iter().enumerate() {
        println!("\n   Context {}:", i + 1);
        println!("     Text: {}", context.text);
        println!("     Level: {:?}", context.level);
        println!("     Relevance: {:.3}", context.relevance_score);
        println!("     Tokens: {}", context.token_count);
    }
    
    println!("\n5. Level distribution:");
    for (level, count) in &response.metadata.level_distribution {
        println!("   {:?}: {} contexts", level, count);
    }
    
    println!("\n=== Example completed successfully! ===");
    
    Ok(())
}