use super::{Message, ScriptConfig, ScriptEngine};
use anyhow::Result;
use extism::{Function, Manifest, Plugin, Val, ValType, Wasm, WasmMetadata, convert::Json};

use reflow_actor::MemoryState;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use base64;

pub struct ExtismEngine {
    pub(crate) manifest: Option<Manifest>,
    pub(crate) config: Option<ScriptConfig>,
}

impl ExtismEngine {
    pub fn new() -> Self {
        Self {
            manifest: None,
            config: None,
        }
    }

    // Create host functions for the Extism plugin
    fn create_host_functions(
        &self,
        outports_sender: flume::Sender<HashMap<String, Message>>,
        state: Arc<parking_lot::Mutex<dyn reflow_actor::ActorState>>,
    ) -> Vec<Function> {
        let mut functions = Vec::new();

        // 1. Create send_output host function

        let outports_sender_clone = outports_sender.clone();
        let user_data = extism::UserData::Rust(Arc::new(Mutex::new(())));

        let send_output = Function::new(
            "__send_output",
            vec![ValType::I64],
            vec![],
            user_data.clone(),
            move |plugin, args, _ret, _data| {
                let output =
                    plugin.memory_get_val::<Json<HashMap<String, serde_json::Value>>>(&args[0])?;
                // Convert output to HashMap<String, Message>
                let output_map = output
                    .into_inner()
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone().into()))
                    .collect();

                outports_sender_clone.send(output_map)?;
                Ok(())
            },
        );

        functions.push(send_output);

        // 2. Create get_state host function
        let state_clone = state.clone();

        let user_data = extism::UserData::Rust(Arc::new(Mutex::new(serde_json::Value::Null)));
        let get_state = Function::new(
            "__get_state",
            vec![ValType::I64],
            vec![ValType::I64],
            user_data,
            move |plugin, args, ret, _data| {
                // Original format: key as string (for Rust WASM SDK compatibility)
                let key = plugin.memory_get_val::<String>(&args[0])?;

                // Get the state value from the actor state
                let state_value = {
                    let state_guard = state_clone.lock();
                    if let Some(memory_state) = state_guard.as_any().downcast_ref::<MemoryState>() {
                        memory_state
                            .get(&key)
                            .cloned()
                            .unwrap_or(serde_json::Value::Null)
                    } else {
                        serde_json::Value::Null
                    }
                };

                // Store the result in plugin memory and return the pointer
                let json_value = Json::from(state_value);
                let ptr = plugin.memory_new(json_value)?;

                ret[0] = Val::I64(ptr.offset() as i64);

                Ok(())
            },
        );

        functions.push(get_state);

        // 3. Create set_state host function
        let state_clone = state.clone();

        let user_data = extism::UserData::Rust(Arc::new(Mutex::new(())));
        let set_state = Function::new(
            "__set_state",
            vec![ValType::I64, ValType::I64],
            vec![],
            user_data.clone(),
            move |plugin, args, _ret, _data| {
                // Original Rust WASM SDK format: separate key and value arguments
                let key = plugin.memory_get_val::<String>(&args[0])?;
                let value = plugin.memory_get_val::<Json<serde_json::Value>>(&args[1])?;
                let state_value = value.into_inner();

                // Set the state value
                let mut state_guard = state_clone.lock();
                if let Some(memory_state) = state_guard.as_mut_any().downcast_mut::<MemoryState>() {
                    memory_state.insert(&key, state_value);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Unable to access state"))
                }
            },
        );

        functions.push(set_state);

        functions
    }
}

// Implement Clone for ExtismEngine
impl Clone for ExtismEngine {
    fn clone(&self) -> Self {
        Self {
            manifest: self.manifest.clone(),
            config: self.config.clone(),
        }
    }
}

unsafe impl Send for ExtismEngine {}
unsafe impl Sync for ExtismEngine {}

#[async_trait::async_trait]
impl ScriptEngine for ExtismEngine {
    async fn init(&mut self, config: &ScriptConfig) -> Result<()> {
        // Store config for potential reuse
        self.config = Some(config.clone());

        let wasm = Wasm::Data {
            data: config.source.clone(),
            meta: WasmMetadata::default(),
        };
        let manifest = Manifest::default().with_wasm(wasm).with_allowed_host("*");

        self.manifest = Some(manifest);

        Ok(())
    }

    async fn call(
        &mut self,
        context: &crate::context::ScriptContext,
    ) -> Result<HashMap<String, Message>> {
        // Store the outports sender in context_senders
        let (outports_sender, _) = context.outports.clone();

        // Create all host functions
        let host_functions = self.create_host_functions(outports_sender, context.state.clone());

        let manifest = self.manifest.as_mut().expect("Manifest is missing");

        let mut plugin = Plugin::new(manifest, host_functions, true)?;

        // Get the config with the entry point
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing configuration"))?;

        // Get current state from the actor
        let current_state = if let Some(memory_state) = context.state.lock().as_any().downcast_ref::<reflow_actor::MemoryState>() {
            // Convert the HashMap to a JSON object
            let state_map: serde_json::Map<String, serde_json::Value> = memory_state.0
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            serde_json::Value::Object(state_map)
        } else {
            serde_json::json!({})
        };

        // Convert ScriptContext to ActorContext format for the plugin
        let actor_context = serde_json::json!({
            "payload": context.inputs.iter().map(|(k, v)| {
                // Convert Message to the plugin's message format
                let msg_value = match v {
                    Message::Flow => json!({"type": "Flow", "data": null}),
                    Message::Event(e) => json!({"type": "Event", "data": e}),
                    Message::Boolean(b) => json!({"type": "Boolean", "data": b}),
                    Message::Integer(i) => json!({"type": "Integer", "data": i}),
                    Message::Float(f) => json!({"type": "Float", "data": f}),
                    Message::String(s) => json!({"type": "String", "data": s}),
                    Message::Object(o) => json!({"type": "Object", "data": o}),
                    Message::Array(a) => {
                        // Convert EncodableValue objects to their JSON representation
                        let json_array: Vec<serde_json::Value> = a.iter()
                            .map(|encodable_value| encodable_value.clone().into())
                            .collect();
                        json!({"type": "Array", "data": json_array})
                    },
                    Message::Stream(s) => json!({"type": "Stream", "data": s.as_ref().clone()}),
                    Message::Optional(o) => json!({"type": "Optional", "data": o}),
                    Message::Any(a) => json!({"type": "Any", "data": a}),
                    Message::Error(e) => json!({"type": "Error", "data": e}),
                    Message::Encoded(e) => {
                        // Encoded messages are handled as binary data
                        json!({"type": "Stream", "data": e.as_ref().clone()})
                    },
                    Message::RemoteReference { network_id, actor_id, port } => {
                        json!({"type": "Any", "data": {
                            "network_id": network_id,
                            "actor_id": actor_id,
                            "port": port
                        }})
                    },
                    Message::NetworkEvent { event_type, data } => {
                        json!({"type": "Event", "data": {
                            "event_type": event_type,
                            "data": data
                        }})
                    },
                };
                (k.clone(), msg_value)
            }).collect::<HashMap<_, _>>(),
            "config": {
                "node_id": context.config.node.id.clone(),
                "component": context.config.node.component.clone(),
                "resolved_env": context.config.resolved_env.clone(),
                "config": context.config.config.clone(),
                "namespace": context.config.namespace.clone(),
            },
            "state": current_state
        });

        // Call the plugin function with ActorContext
        let result = plugin
            .call::<Json<serde_json::Value>, Json<serde_json::Value>>(
                &config.entry_point,
                Json(actor_context),
            )?;

        // Parse the ActorResult from the plugin
        let mut actor_result: serde_json::Value = result.into_inner();
        
        // Check if the result is a base64-encoded string (from Go SDK)
        if let Some(base64_str) = actor_result.as_str() {
            // println!("Detected base64 string from plugin, decoding...");
            // Try to decode the base64 string
            use base64::{Engine as _, engine::general_purpose};
            match general_purpose::STANDARD.decode(base64_str) {
                Ok(decoded_bytes) => {
                    // Parse the decoded bytes as JSON
                    match serde_json::from_slice::<serde_json::Value>(&decoded_bytes) {
                        Ok(decoded_json) => {
                            // println!("Successfully decoded base64 to JSON");
                            actor_result = decoded_json;
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!("Failed to parse decoded base64 as JSON: {}", e));
                        }
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Failed to decode base64 string: {}", e));
                }
            }
        }
        
        // println!("Actor result: {:?}", actor_result);
        
        // Extract outputs from ActorResult
        let outputs = actor_result
            .get("outputs")
            .and_then(|o| o.as_object())
            .ok_or_else(|| anyhow::anyhow!("Invalid ActorResult: missing outputs"))?;

        // Convert outputs back to Message format
        let mut output_messages = HashMap::new();
        for (port, msg_value) in outputs {
            if let Some(msg_obj) = msg_value.as_object() {
                if let (Some(msg_type), Some(data)) = (msg_obj.get("type").and_then(|t| t.as_str()), msg_obj.get("data")) {
                    let message = match msg_type {
                        "Flow" => Message::Flow,
                        "Event" => Message::Event(data.clone().into()),
                        "Boolean" => Message::Boolean(data.as_bool().unwrap_or_default()),
                        "Integer" => Message::Integer(data.as_i64().unwrap_or_default()),
                        "Float" => Message::Float(data.as_f64().unwrap_or_default()),
                        "String" => Message::String(Arc::new(data.as_str().unwrap_or_default().to_string())),
                        "Object" => Message::Object(Arc::new(data.clone().into())),
                        "Array" => Message::Array(Arc::new(data.as_array().cloned().unwrap_or_default().into_iter().map(Into::into).collect())),
                        "Stream" => {
                            // Handle stream data as Vec<u8>
                            if let Ok(bytes) = serde_json::from_value::<Vec<u8>>(data.clone()) {
                                Message::Stream(Arc::new(bytes))
                            } else {
                                Message::Stream(Arc::new(vec![]))
                            }
                        },
                        "Optional" => {
                            if data.is_null() {
                                Message::Optional(None)
                            } else {
                                Message::Optional(Some(Arc::new(data.clone().into())))
                            }
                        },
                        "Any" => Message::Any(Arc::new(data.clone().into())),
                        "Error" => Message::Error(Arc::new(data.as_str().unwrap_or_default().to_string())),
                        _ => continue,
                    };
                    output_messages.insert(port.clone(), message);
                }
            }
        }

        // Update state if provided in the result
        if let Some(new_state) = actor_result.get("state") {
            if let Some(memory_state) = context.state.lock().as_mut_any().downcast_mut::<reflow_actor::MemoryState>() {
                // Update state by clearing and inserting new values
                if let Some(state_obj) = new_state.as_object() {
                    memory_state.clear();
                    for (key, value) in state_obj {
                        memory_state.insert(key, value.clone());
                    }
                }
            }
        }

        Ok(output_messages)
    }

    async fn cleanup(&mut self) -> Result<()> {
        self.manifest = None;

        Ok(())
    }
}

// Implement Drop to ensure cleanup when the engine is dropped
impl Drop for ExtismEngine {
    fn drop(&mut self) {
        self.manifest = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ScriptEnvironment, ScriptRuntime};
    use parking_lot::Mutex;
    use reflow_actor::{MemoryState, Port};
    use std::sync::Arc;

    fn test_config(source: Vec<u8>) -> ScriptConfig {
        ScriptConfig {
            environment: ScriptEnvironment::SYSTEM,
            runtime: ScriptRuntime::Extism,
            source,
            entry_point: "process".to_string(), // Default Extism entry point
            packages: None,
        }
    }

    #[tokio::test]
    async fn test_wasm_actor() -> Result<()> {
        let wasm_binary = include_bytes!("../../../crates/reflow_wasm/examples/counter_actor/target/wasm32-unknown-unknown/release/counter_actor.wasm");

        let mut engine = ExtismEngine::new();
        let config = test_config(wasm_binary.to_vec());

        engine.init(&config).await?;

        // Create state and ports for context
        let state = Arc::new(Mutex::new(MemoryState::default()));
        let (sender, receiver) = flume::unbounded();
        let outports: Port = (sender, receiver);

        // Test simple function call
        let mut inputs = HashMap::new();
        inputs.insert(
            "increment".to_string(),
            Message::Flow,
        );

        // Create a dummy actor config for testing
        let node = reflow_actor::types::GraphNode {
            id: "test_node".to_string(),
            component: "TestComponent".to_string(),
            metadata: None,
            ..Default::default()
        };
        
        let actor_config = reflow_actor::ActorConfig {
            node: node.clone(),
            resolved_env: HashMap::new(),
            config: HashMap::new(),
            namespace: None,
        };

        let context = crate::context::ScriptContext::new(
            config.entry_point,
            inputs.clone(),
            state.clone(),
            outports.clone(),
            actor_config,
        );

        let result = engine.call(&context).await?;
       
        // Verify the result
        assert!(
            result.contains_key("info"),
            "Result should contain 'info' key"
        );
        assert!(matches!(result["info"], Message::Object(..)));
        assert!(
            result.contains_key("count"),
            "Result should contain 'count' key"
        );
        assert_eq!(result["count"], Message::Integer(1));
        

        engine.cleanup().await?;
        Ok(())
    }
}
