//! Message handler implementation

use super::messages::*;
use crate::error::Result;
use crate::hirag::ContextManager;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, error};

/// Trait for handling messages
#[async_trait]
pub trait MessageHandler: Send + Sync {
    /// Handle incoming message
    async fn handle_message(&self, message: Message) -> Result<Option<Message>>;
}

/// Default message handler implementation
pub struct DefaultMessageHandler {
    context_manager: Arc<dyn ContextManager>,
}

impl DefaultMessageHandler {
    pub fn new(context_manager: Arc<dyn ContextManager>) -> Self {
        Self { context_manager }
    }
    
    /// Create response message
    fn create_response(&self, original: &Message, payload: MessagePayload) -> Message {
        Message {
            id: uuid::Uuid::new_v4(),
            version: crate::protocol::PROTOCOL_VERSION.to_string(),
            message_type: match &payload {
                MessagePayload::ContextResponse(_) => MessageType::ContextResponse,
                MessagePayload::Acknowledgment(_) => MessageType::Acknowledgment,
                MessagePayload::Error(_) => MessageType::Error,
                _ => MessageType::Acknowledgment,
            },
            timestamp: chrono::Utc::now().timestamp(),
            sender: "hirag_manager".to_string(),
            recipient: Some(original.sender.clone()),
            payload,
            metadata: std::collections::HashMap::new(),
        }
    }
    
    /// Create error response
    fn create_error_response(&self, original: &Message, code: String, message: String) -> Message {
        self.create_response(
            original,
            MessagePayload::Error(ErrorPayload {
                code,
                message,
                details: None,
            }),
        )
    }
}

#[async_trait]
impl MessageHandler for DefaultMessageHandler {
    async fn handle_message(&self, message: Message) -> Result<Option<Message>> {
        debug!("Handling message type: {:?}", message.message_type);
        
        match &message.payload {
            MessagePayload::ContextRequest(request) => {
                match self.context_manager.retrieve_context(request.clone()).await {
                    Ok(response) => {
                        let reply = self.create_response(
                            &message,
                            MessagePayload::ContextResponse(response),
                        );
                        Ok(Some(reply))
                    }
                    Err(e) => {
                        error!("Context retrieval failed: {}", e);
                        let reply = self.create_error_response(
                            &message,
                            "RETRIEVAL_FAILED".to_string(),
                            e.to_string(),
                        );
                        Ok(Some(reply))
                    }
                }
            }
            MessagePayload::ContextStore(store_payload) => {
                match self.context_manager.store_context(
                    &store_payload.text,
                    store_payload.level,
                    store_payload.metadata.clone(),
                ).await {
                    Ok(_id) => {
                        let reply = self.create_response(
                            &message,
                            MessagePayload::Acknowledgment(AckPayload {
                                message_id: message.id,
                                status: AckStatus::Success,
                            }),
                        );
                        Ok(Some(reply))
                    }
                    Err(e) => {
                        error!("Context storage failed: {}", e);
                        let reply = self.create_error_response(
                            &message,
                            "STORAGE_FAILED".to_string(),
                            e.to_string(),
                        );
                        Ok(Some(reply))
                    }
                }
            }
            MessagePayload::Heartbeat(_) => {
                // Respond with heartbeat
                let reply = self.create_response(
                    &message,
                    MessagePayload::Heartbeat(HeartbeatPayload {
                        sequence: 0,
                        status: SystemStatus {
                            healthy: true,
                            uptime_secs: 0,
                            active_connections: 0,
                        },
                    }),
                );
                Ok(Some(reply))
            }
            _ => {
                // For other message types, just acknowledge
                let reply = self.create_response(
                    &message,
                    MessagePayload::Acknowledgment(AckPayload {
                        message_id: message.id,
                        status: AckStatus::Success,
                    }),
                );
                Ok(Some(reply))
            }
        }
    }
}