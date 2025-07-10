//! WebSocket protocol definitions for tracing communication

pub mod messages;

pub use messages::*;

/// Protocol version for compatibility checking
pub const PROTOCOL_VERSION: &str = "1.0.0";

/// Maximum message size (16MB)
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// WebSocket sub-protocol identifier
pub const WEBSOCKET_PROTOCOL: &str = "reflow-tracing-v1";
