//! Example demonstrating the communication protocol

use context_manager::prelude::*;
use context_manager::protocol::{
    MessagePayload, MessageType, JsonCodec,
};
use context_manager::protocol::messages::ContextStorePayload;
use context_manager::protocol::handler::{MessageHandler, DefaultMessageHandler};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize observability
    context_manager::observability::init_observability("info", "pretty");
    
    println!("=== Protocol Communication Example ===\n");
    
    // Load configuration
    let mut config = Config::default_config();
    config.embedding.api_token = secrecy::Secret::new(
        std::env::var("CHUTES_API_TOKEN")
            .expect("CHUTES_API_TOKEN environment variable not set")
    );
    
    println!("1. Initializing system...");
    
    // Initialize components
    let embedding_client = std::sync::Arc::new(
        EmbeddingClient::new(config.embedding.clone())?
    );
    
    let vector_db = std::sync::Arc::new(
        VectorDbClient::new(config.vector_db.clone()).await?
    );
    
    vector_db.initialize_collections().await?;
    
    let hirag_manager = std::sync::Arc::new(
        HiRAGManager::new(
            config.hirag.clone(),
            embedding_client.clone(),
            vector_db.clone(),
        ).await?
    );
    
    hirag_manager.initialize().await?;
    println!("   ✓ System initialized\n");
    
    // Create message handler
    let handler = DefaultMessageHandler::new(hirag_manager.clone());
    
    // Create codec
    let codec = JsonCodec;
    
    println!("2. Sending ContextStore message...");
    
    // Create a context store message
    let store_message = Message::new(
        MessageType::ContextStore,
        "example_agent".to_string(),
        MessagePayload::ContextStore(ContextStorePayload {
            text: "User is interested in machine learning".to_string(),
            level: ContextLevel::LongTerm,
            metadata: HashMap::from([
                ("topic".to_string(), serde_json::json!("AI")),
            ]),
        }),
    );
    
    println!("   Message ID: {}", store_message.id);
    
    // Encode message
    let encoded = codec.encode(&store_message)?;
    println!("   Encoded size: {} bytes", encoded.len());
    
    // Decode message
    let decoded = codec.decode(&encoded)?;
    println!("   ✓ Message encoded and decoded successfully");
    
    // Handle message
    let response = handler.handle_message(decoded).await?;
    
    if let Some(reply) = response {
        println!("\n3. Received response:");
        println!("   Type: {:?}", reply.message_type);
        println!("   From: {}", reply.sender);
        
        if let MessagePayload::Acknowledgment(ack) = reply.payload {
            println!("   Status: {:?}", ack.status);
        }
    }
    
    println!("\n4. Sending ContextRequest message...");
    
    // Create a context request message
    let request_message = Message::new(
        MessageType::ContextRequest,
        "example_agent".to_string(),
        MessagePayload::ContextRequest(ContextRequest::new(
            "What is the user interested in?".to_string(),
            1000,
        )),
    );
    
    println!("   Message ID: {}", request_message.id);
    
    // Handle request
    let response = handler.handle_message(request_message).await?;
    
    if let Some(reply) = response {
        println!("\n5. Received context response:");
        println!("   Type: {:?}", reply.message_type);
        
        if let MessagePayload::ContextResponse(ctx_response) = reply.payload {
            println!("   Contexts retrieved: {}", ctx_response.contexts.len());
            println!("   Total tokens: {}", ctx_response.total_tokens);
            println!("   Retrieval time: {}ms", ctx_response.retrieval_time_ms);
            
            for (i, context) in ctx_response.contexts.iter().enumerate() {
                println!("\n   Context {}:", i + 1);
                println!("     {}", context.text);
                println!("     Relevance: {:.3}", context.relevance_score);
            }
        }
    }
    
    println!("\n=== Protocol example completed! ===");
    
    Ok(())
}