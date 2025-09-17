use super::client::WebSocketRpcClient;
use super::types::*;
use crate::script_discovery::types::DiscoveredScriptActor;
use crate::redis_state::RedisActorState;
use reflow_actor::{Actor, ActorBehavior, ActorConfig, ActorContext, ActorLoad, Port, message::{EncodableValue, Message}, ActorState, MemoryState};
use anyhow::Result;
use serde_json::{json, Value};
use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::{Mutex, RwLock};
use tracing::{debug, warn, error};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use std::pin::Pin;
use futures::Future;

/// Script actor that communicates via WebSocket RPC
/// Implements the Actor trait for integration with reflow network
pub struct WebSocketScriptActor {
    pub metadata: DiscoveredScriptActor,
    pub rpc_client: Arc<WebSocketRpcClient>,
    pub redis_url: String,
    pub outputs: Arc<RwLock<Vec<(String, Message)>>>,
    inports_channel: Option<Port>,
    outports_channel: Option<Port>,
    /// Channel to receive async outputs from scripts
    output_receiver: Arc<Mutex<Option<flume::Receiver<ScriptOutput>>>>,
    /// Redis state for persistent actor state
    redis_state: Option<Arc<RedisActorState>>,
}

impl WebSocketScriptActor {
    pub async fn new(
        metadata: DiscoveredScriptActor,
        rpc_client: Arc<WebSocketRpcClient>,
        redis_url: String,
    ) -> Self {
        // Create channel for async outputs
        let (output_tx, output_rx) = flume::unbounded();
        
        // Set the output channel in the RPC client
        rpc_client.set_output_channel(output_tx);
        
        // Try to create Redis state connection
        let redis_state = if !redis_url.is_empty() && redis_url != "none" {
            match RedisActorState::new(
                &redis_url,
                &metadata.workspace_metadata.namespace,
                &metadata.component
            ).await {
                Ok(state) => {
                    debug!("Connected to Redis for actor state: {}", metadata.component);
                    Some(Arc::new(state))
                }
                Err(e) => {
                    warn!("Failed to connect to Redis for actor {}: {}. State persistence disabled.", 
                        metadata.component, e);
                    None
                }
            }
        } else {
            debug!("Redis URL not provided, state persistence disabled for {}", metadata.component);
            None
        };
        
        Self {
            metadata,
            rpc_client,
            redis_url,
            outputs: Arc::new(RwLock::new(Vec::new())),
            inports_channel: None,
            outports_channel: None,
            output_receiver: Arc::new(Mutex::new(Some(output_rx))),
            redis_state,
        }
    }
    
    /// Convert Reflow message to JSON using built-in conversion
    /// Message already implements Into<Value> which handles serialization
    fn message_to_json(&self, message: &Message) -> Value {
        // Use the built-in Message to Value conversion
        // This preserves the serde tagging that scripts expect
        message.clone().into()
    }
    
    /// Convert JSON to Reflow message using built-in conversion
    fn json_to_message(&self, value: &Value) -> Result<Message> {
        // Use the built-in From<Value> for Message conversion
        // This handles the serde tagged format automatically
        Ok(Message::from(value.clone()))
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
    
    /// Process a message through the script actor
    pub async fn process_message(&mut self, inputs: HashMap<String, Message>) -> Result<HashMap<String, Message>> {
        debug!("Processing message in script actor: {}", self.metadata.component);
        println!("DEBUG: process_message called for {}", self.metadata.component);
        
        // Ensure connected
        println!("DEBUG: Ensuring connection...");
        self.rpc_client.ensure_connected().await?;
        println!("DEBUG: Connection ensured");
        
        // Convert inputs to JSON
        let mut payload = serde_json::Map::new();
        for (port, msg) in inputs {
            payload.insert(port, self.message_to_json(&msg));
        }
        
        // Create context for script
        let context = ScriptActorContext {
            payload: Value::Object(payload),
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
        println!("DEBUG: Calling RPC with context: {:?}", context);
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
                
                // Convert to HashMap
                let mut output_map = HashMap::new();
                for (port, msg) in self.outputs.read().iter() {
                    output_map.insert(port.clone(), msg.clone());
                }
                Ok(output_map)
            }
            Err(e) => {
                error!("RPC call failed: {}", e);
                Err(e)
            }
        }
    }
}

impl Actor for WebSocketScriptActor {
    fn get_behavior(&self) -> ActorBehavior {
        let rpc_client = self.rpc_client.clone();
        let redis_url = self.redis_url.clone();
        let metadata = self.metadata.clone();
        let outputs = self.outputs.clone();
        
        Box::new(move |context: ActorContext| {
            let rpc_client = rpc_client.clone();
            let redis_url = redis_url.clone();
            let metadata = metadata.clone();
            let _outputs = outputs.clone();
            let payload = context.payload;
            
            Box::pin(async move {
                debug!("Processing message in WebSocket script actor: {}", metadata.component);
                
                // Ensure connected
                if let Err(e) = rpc_client.ensure_connected().await {
                    error!("Failed to connect to WebSocket RPC: {}", e);
                    return Err(e);
                }
                
                // Convert inputs to JSON using Message's built-in conversion
                let mut json_payload = serde_json::Map::new();
                for (port, msg) in &payload {
                    // Message already implements Into<Value>
                    let json_value: Value = msg.clone().into();
                    json_payload.insert(port.clone(), json_value);
                }
                
                // Create context for script
                let script_context = ScriptActorContext {
                    payload: Value::Object(json_payload),
                    config: json!({}),
                    state: StateContext {
                        namespace: metadata.workspace_metadata.namespace.clone(),
                        actor_id: metadata.component.clone(),
                        redis_url: redis_url.clone(),
                    },
                    actor_id: metadata.component.clone(),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                };
                
                // Call script via RPC
                match rpc_client.call("process", json!(script_context)).await {
                    Ok(result) => {
                        // Check for error
                        if let Some(error) = result.get("error") {
                            error!("Script execution error: {}", error);
                            return Err(anyhow::anyhow!("Script execution failed: {}", error));
                        }
                        
                        // Process outputs
                        let mut output_map = HashMap::new();
                        if let Some(outputs_value) = result.get("outputs") {
                            if let Some(outputs_obj) = outputs_value.as_object() {
                                for (port, value) in outputs_obj {
                                    // Convert JSON back to Message
                                    // For simplicity, using Message::from(Value)
                                    let msg = Message::from(value.clone());
                                    output_map.insert(port.clone(), msg);
                                }
                            }
                        }
                        
                        Ok(output_map)
                    }
                    Err(e) => {
                        error!("RPC call failed: {}", e);
                        Err(e)
                    }
                }
            })
        })
    }
    
    fn get_outports(&self) -> Port {
        if let Some(port) = &self.outports_channel {
            port.clone()
        } else {
            let (tx, rx) = flume::unbounded();
            (tx, rx)
        }
    }
    
    fn get_inports(&self) -> Port {
        if let Some(port) = &self.inports_channel {
            port.clone()
        } else {
            let (tx, rx) = flume::unbounded();
            (tx, rx)
        }
    }
    
    fn create_process(
        &self,
        actor_config: ActorConfig,
        _tracing_integration: Option<crate::tracing::TracingIntegration>,
    ) -> Pin<Box<dyn Future<Output = ()> + 'static + Send>> {
        let inports = self.get_inports();
        let behavior = self.get_behavior();
        let state: Arc<Mutex<dyn ActorState>> = Arc::new(Mutex::new(MemoryState::default()));
        let outports = self.get_outports();
        let load_count = Arc::new(Mutex::new(ActorLoad::new(0)));
        let output_receiver = self.output_receiver.clone();
        let metadata = self.metadata.clone();
        
        Box::pin(async move {
            // Spawn a task to handle async outputs from scripts
            let outports_async = outports.clone();
            let output_rx = output_receiver.lock().take();
            if let Some(rx) = output_rx {
                tokio::spawn(async move {
                    while let Ok(script_output) = rx.recv_async().await {
                        debug!("Routing async output from script {} on port {}", 
                            script_output.actor_id, script_output.port);
                        
                        // Convert the JSON data to a Message
                        let msg = Message::from(script_output.data);
                        let mut output = HashMap::new();
                        output.insert(script_output.port, msg);
                        
                        // Send through the outports channel
                        if let Err(e) = outports_async.0.send_async(output).await {
                            error!("Failed to send async output: {}", e);
                            break;
                        }
                    }
                    debug!("Async output handler terminated for {}", metadata.component);
                });
            }
            
            // Main message processing loop
            while let Ok(payload) = inports.1.recv_async().await {
                let context = ActorContext::new(
                    payload,
                    outports.clone(),
                    state.clone(),
                    actor_config.clone(),
                    load_count.clone(),
                );
                
                match behavior(context).await {
                    Ok(result) => {
                        // Only send if there are results (sync outputs)
                        if !result.is_empty() {
                            if let Err(e) = outports.0.send_async(result).await {
                                error!("Failed to send output: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Actor behavior failed: {}", e);
                        // Send error message
                        let mut error_output = HashMap::new();
                        error_output.insert("error".to_string(), Message::Error(Arc::new(e.to_string())));
                        if let Err(send_err) = outports.0.send_async(error_output).await {
                            error!("Failed to send error output: {}", send_err);
                        }
                        break;
                    }
                }
            }
        })
    }
}