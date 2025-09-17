use super::client::WebSocketRpcClient;
use super::types::*;
use crate::script_discovery::types::DiscoveredScriptActor;
use reflow_actor::message::Message;
use anyhow::Result;
use serde_json::{json, Value};
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{debug, warn, error};

/// A wrapper for script actors that communicates via WebSocket RPC
/// This is not a full Actor implementation but a helper for script execution
pub struct ScriptActorWrapper {
    pub metadata: DiscoveredScriptActor,
    pub rpc_client: Arc<WebSocketRpcClient>,
    pub redis_url: String,
    pub outputs: Arc<RwLock<Vec<(String, Message)>>>,
}

impl ScriptActorWrapper {
    pub fn new(
        metadata: DiscoveredScriptActor,
        rpc_client: Arc<WebSocketRpcClient>,
        redis_url: String,
    ) -> Self {
        Self {
            metadata,
            rpc_client,
            redis_url,
            outputs: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Process a message through the script actor
    pub async fn process_message(&mut self, input: Message) -> Result<Vec<(String, Message)>> {
        debug!("Processing message in script actor: {}", self.metadata.component);
        
        // Ensure connected
        self.rpc_client.ensure_connected().await?;
        
        // Create context for script
        let context = ScriptActorContext {
            payload: self.message_to_json(&input),
            config: json!({}),
            state: StateContext {
                namespace: self.metadata.workspace_metadata.namespace.clone(),
                actor_id: self.metadata.component.clone(),
                redis_url: self.redis_url.clone(),
            },
            actor_id: self.metadata.component.clone(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        };
        
        // Call script via RPC
        match self.rpc_client.call("process", json!(context)).await {
            Ok(result) => {
                // Check for error
                if let Some(error) = result.get("error") {
                    error!("Script execution error: {}", error);
                    return Err(anyhow::anyhow!("Script execution failed: {}", error));
                }
                
                // Process outputs
                if let Some(outputs) = result.get("outputs") {
                    self.process_outputs(outputs.clone())?;
                }
                
                Ok(self.outputs.read().clone())
            }
            Err(e) => {
                error!("RPC call failed: {}", e);
                Err(e)
            }
        }
    }
    
    /// Convert Reflow message to JSON
    fn message_to_json(&self, message: &Message) -> Value {
        match message {
            Message::Integer(i) => json!({
                "type": "integer",
                "value": i
            }),
            Message::Float(f) => json!({
                "type": "float",
                "value": f
            }),
            Message::String(s) => json!({
                "type": "string",
                "value": s
            }),
            Message::Boolean(b) => json!({
                "type": "boolean",
                "value": b
            }),
            Message::Array(arr) => json!({
                "type": "array",
                "value": arr.iter().map(|m| self.message_to_json(m)).collect::<Vec<_>>()
            }),
            Message::Object(obj) => {
                let mut map = serde_json::Map::new();
                for (k, v) in obj {
                    map.insert(k.clone(), self.message_to_json(v));
                }
                json!({
                    "type": "object",
                    "value": map
                })
            }
            Message::Flow => json!({
                "type": "flow",
                "value": null
            }),
            _ => json!({
                "type": "any",
                "value": null
            })
        }
    }
    
    /// Convert JSON to Reflow message
    fn json_to_message(&self, value: &Value) -> Result<Message> {
        if let Some(obj) = value.as_object() {
            if let Some(type_str) = obj.get("type").and_then(|v| v.as_str()) {
                let val = obj.get("value");
                
                match type_str {
                    "integer" => {
                        if let Some(i) = val.and_then(|v| v.as_i64()) {
                            return Ok(Message::Integer(i));
                        }
                    }
                    "float" => {
                        if let Some(f) = val.and_then(|v| v.as_f64()) {
                            return Ok(Message::Float(f.into()));
                        }
                    }
                    "string" => {
                        if let Some(s) = val.and_then(|v| v.as_str()) {
                            return Ok(Message::String(s.to_string()));
                        }
                    }
                    "boolean" => {
                        if let Some(b) = val.and_then(|v| v.as_bool()) {
                            return Ok(Message::Boolean(b));
                        }
                    }
                    "array" => {
                        if let Some(arr) = val.and_then(|v| v.as_array()) {
                            let mut messages = Vec::new();
                            for item in arr {
                                messages.push(self.json_to_message(item)?);
                            }
                            return Ok(Message::Array(messages));
                        }
                    }
                    "object" => {
                        if let Some(obj) = val.and_then(|v| v.as_object()) {
                            let mut map = std::collections::HashMap::new();
                            for (k, v) in obj {
                                map.insert(k.clone(), self.json_to_message(v)?);
                            }
                            return Ok(Message::Object(map));
                        }
                    }
                    "flow" => {
                        return Ok(Message::Flow);
                    }
                    _ => {}
                }
            }
        }
        
        // Fallback: treat as raw value
        Ok(Message::Object(std::collections::HashMap::from([
            ("value".to_string(), Message::String(value.to_string()))
        ])))
    }
    
    /// Process outputs from script execution
    fn process_outputs(&self, outputs: Value) -> Result<()> {
        if let Some(outputs_obj) = outputs.as_object() {
            let mut output_messages = Vec::new();
            
            for (port, value) in outputs_obj {
                match self.json_to_message(value) {
                    Ok(message) => {
                        output_messages.push((port.clone(), message));
                    }
                    Err(e) => {
                        warn!("Failed to convert output for port {}: {}", port, e);
                    }
                }
            }
            
            // Store outputs
            *self.outputs.write() = output_messages;
        }
        
        Ok(())
    }
}