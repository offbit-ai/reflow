use serde::{Deserialize, Serialize};
use serde_json::Value;

/// RPC request message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: Value,
}

impl RpcRequest {
    pub fn new(id: String, method: String, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method,
            params,
        }
    }
}

/// RPC response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

/// RPC error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Actor context sent to scripts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptActorContext {
    pub payload: Value,
    pub config: Value,
    pub state: StateContext,
    pub actor_id: String,
    pub timestamp: u64,
}

/// State context for script actors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateContext {
    pub namespace: String,
    pub actor_id: String,
    pub redis_url: String,
}

/// Script execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptExecutionResult {
    pub outputs: Option<Value>,
    pub error: Option<String>,
    pub state_updates: Option<Vec<StateUpdate>>,
}

/// State update operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateUpdate {
    pub operation: String,
    pub key: String,
    pub value: Option<Value>,
}

/// RPC notification (no response expected)
/// Used for async output messages from scripts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

/// Async output message from script
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptOutput {
    pub actor_id: String,
    pub port: String,
    pub data: Value,
    pub timestamp: u64,
}

/// Types of messages that can be received over WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WebSocketMessage {
    Response(RpcResponse),
    Notification(RpcNotification),
}