//! Communication protocol for agent interactions

pub mod messages;
pub mod codec;
pub mod handler;
pub mod auth;

pub use messages::{Message, MessageType, MessagePayload};
pub use codec::{Codec, JsonCodec, MessagePackCodec};
pub use handler::MessageHandler;

/// Protocol version
pub const PROTOCOL_VERSION: &str = "1.0.0";