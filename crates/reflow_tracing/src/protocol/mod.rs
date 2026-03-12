//! WebSocket protocol definitions for tracing communication

#[allow(dead_code)]
pub mod messages;

/// Protocol version for compatibility checking
#[allow(dead_code)]
pub const PROTOCOL_VERSION: &str = "1.0.0";

/// Maximum message size (16MB)
#[allow(dead_code)]
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// WebSocket sub-protocol identifier
#[allow(dead_code)]
pub const WEBSOCKET_PROTOCOL: &str = "reflow-tracing-v1";
