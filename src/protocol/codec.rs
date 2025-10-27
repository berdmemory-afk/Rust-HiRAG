//! Message encoding and decoding with security guards

use super::messages::Message;
use crate::error::{ProtocolError, Result};
use bytes::Bytes;

/// Maximum message size (10 MB)
const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

/// Trait for message codecs
pub trait Codec: Send + Sync {
    /// Encode message to bytes
    fn encode(&self, message: &Message) -> Result<Bytes>;
    
    /// Decode bytes to message
    fn decode(&self, data: &[u8]) -> Result<Message>;
    
    /// Get codec name
    fn name(&self) -> &str;
    
    /// Get maximum message size
    fn max_size(&self) -> usize {
        MAX_MESSAGE_SIZE
    }
}

/// JSON codec implementation with size guards
pub struct JsonCodec;

impl Codec for JsonCodec {
    fn encode(&self, message: &Message) -> Result<Bytes> {
        let json = serde_json::to_vec(message)
            .map_err(|e| ProtocolError::EncodingError(e.to_string()))?;
        
        // Check encoded size
        if json.len() > MAX_MESSAGE_SIZE {
            return Err(ProtocolError::MessageTooLarge {
                size: json.len(),
                max_size: MAX_MESSAGE_SIZE,
            }.into());
        }
        
        Ok(Bytes::from(json))
    }
    
    fn decode(&self, data: &[u8]) -> Result<Message> {
        // Check size before decoding
        if data.len() > MAX_MESSAGE_SIZE {
            return Err(ProtocolError::MessageTooLarge {
                size: data.len(),
                max_size: MAX_MESSAGE_SIZE,
            }.into());
        }
        
        // Deserialize (serde_json has built-in recursion limits)
        let message = serde_json::from_slice(data)
            .map_err(|e| ProtocolError::DecodingError(e.to_string()))?;
        
        Ok(message)
    }
    
    fn name(&self) -> &str {
        "json"
    }
}

/// MessagePack codec implementation with size guards
pub struct MessagePackCodec;

impl Codec for MessagePackCodec {
    fn encode(&self, message: &Message) -> Result<Bytes> {
        let msgpack = rmp_serde::to_vec(message)
            .map_err(|e| ProtocolError::EncodingError(e.to_string()))?;
        
        // Check encoded size
        if msgpack.len() > MAX_MESSAGE_SIZE {
            return Err(ProtocolError::MessageTooLarge {
                size: msgpack.len(),
                max_size: MAX_MESSAGE_SIZE,
            }.into());
        }
        
        Ok(Bytes::from(msgpack))
    }
    
    fn decode(&self, data: &[u8]) -> Result<Message> {
        // Check size before decoding
        if data.len() > MAX_MESSAGE_SIZE {
            return Err(ProtocolError::MessageTooLarge {
                size: data.len(),
                max_size: MAX_MESSAGE_SIZE,
            }.into());
        }
        
        // Deserialize with standard decoder
        let message = rmp_serde::from_slice(data)
            .map_err(|e| ProtocolError::DecodingError(e.to_string()))?;
        
        Ok(message)
    }
    
    fn name(&self) -> &str {
        "messagepack"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::messages::*;
    
    #[test]
    fn test_json_codec_roundtrip() {
        let codec = JsonCodec;
        
        let original = Message::new(
            MessageType::Heartbeat,
            "test_sender".to_string(),
            MessagePayload::Heartbeat(HeartbeatPayload {
                sequence: 1,
                status: SystemStatus {
                    healthy: true,
                    uptime_secs: 100,
                    active_connections: 5,
                },
            }),
        );
        
        let encoded = codec.encode(&original).unwrap();
        let decoded = codec.decode(&encoded).unwrap();
        
        assert_eq!(original.id, decoded.id);
        assert_eq!(original.sender, decoded.sender);
    }
    
    #[test]
    fn test_messagepack_codec_roundtrip() {
        let codec = MessagePackCodec;
        
        let original = Message::new(
            MessageType::Heartbeat,
            "test_sender".to_string(),
            MessagePayload::Heartbeat(HeartbeatPayload {
                sequence: 1,
                status: SystemStatus {
                    healthy: true,
                    uptime_secs: 100,
                    active_connections: 5,
                },
            }),
        );
        
        let encoded = codec.encode(&original).unwrap();
        let decoded = codec.decode(&encoded).unwrap();
        
        assert_eq!(original.id, decoded.id);
        assert_eq!(original.sender, decoded.sender);
    }
    
    #[test]
    fn test_size_limit_json() {
        let codec = JsonCodec;
        
        // Create a message that's too large
        let large_data = vec![0u8; MAX_MESSAGE_SIZE + 1];
        
        let result = codec.decode(&large_data);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_size_limit_messagepack() {
        let codec = MessagePackCodec;
        
        // Create a message that's too large
        let large_data = vec![0u8; MAX_MESSAGE_SIZE + 1];
        
        let result = codec.decode(&large_data);
        assert!(result.is_err());
    }
}