//! Message types and structures for the protocol

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use crate::hirag::{ContextRequest, ContextResponse};
use crate::vector_db::ContextLevel;

/// Message envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message ID
    pub id: Uuid,
    
    /// Protocol version
    pub version: String,
    
    /// Message type
    pub message_type: MessageType,
    
    /// Timestamp
    pub timestamp: i64,
    
    /// Sender identifier
    pub sender: String,
    
    /// Recipient identifier (optional)
    pub recipient: Option<String>,
    
    /// Message payload
    pub payload: MessagePayload,
    
    /// Optional metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Message types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum MessageType {
    ContextRequest,
    ContextResponse,
    ContextStore,
    Acknowledgment,
    Error,
    Heartbeat,
}

/// Message payload variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessagePayload {
    ContextRequest(ContextRequest),
    ContextResponse(ContextResponse),
    ContextStore(ContextStorePayload),
    Acknowledgment(AckPayload),
    Error(ErrorPayload),
    Heartbeat(HeartbeatPayload),
}

/// Payload for storing context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextStorePayload {
    pub text: String,
    pub level: ContextLevel,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Acknowledgment payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckPayload {
    pub message_id: Uuid,
    pub status: AckStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AckStatus {
    Success,
    Partial,
    Failed,
}

/// Error payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

/// Heartbeat payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    pub sequence: u64,
    pub status: SystemStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub healthy: bool,
    pub uptime_secs: u64,
    pub active_connections: usize,
}

impl Message {
    pub fn new(
        message_type: MessageType,
        sender: String,
        payload: MessagePayload,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            version: crate::protocol::PROTOCOL_VERSION.to_string(),
            message_type,
            timestamp: chrono::Utc::now().timestamp(),
            sender,
            recipient: None,
            payload,
            metadata: HashMap::new(),
        }
    }
    
    pub fn with_recipient(mut self, recipient: String) -> Self {
        self.recipient = Some(recipient);
        self
    }
    
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}