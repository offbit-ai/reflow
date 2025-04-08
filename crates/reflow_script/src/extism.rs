use super::{Message, ScriptConfig, ScriptEngine};
use anyhow::Result;
use extism::{Function, Manifest, Plugin, Val, ValType, Wasm, WasmMetadata, convert::Json};

use reflow_network::actor::MemoryState;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

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
        state: Arc<parking_lot::Mutex<dyn reflow_network::actor::ActorState>>,
    ) -> Vec<Function> {
        let mut functions = Vec::new();

        // 1. Create send_output host function

        let outports_sender_clone = outports_sender.clone();
        let user_data = extism::UserData::Rust(Arc::new(Mutex::new(())));

        let send_output = Function::new(
            "__send_output",
            vec![ValType::I64],
            vec![ValType::I64],
            user_data.clone(),
            move |plugin, args, ret, _data| {
                ret[0] = Val::I64(0);
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
            vec![ValType::I64],
            user_data,
            move |plugin, args, ret, _data| {
                let key = plugin.memory_get_val::<String>(&args[0])?;
                let value = plugin.memory_get_val::<Json<serde_json::Value>>(&args[1])?;

                // Extract the value from Json wrapper
                let state_value = value.into_inner();
                ret[0] = Val::I64(0);
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
// impl Clone for ExtismEngine {
//     fn clone(&self) -> Self {
//         Self {
//             plugin: self.plugin.clone(),
//             config: self.config.clone(),
//             context_senders: self.context_senders.clone(),
//         }
//     }
// }

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

        // Convert script context to serializable format
        let serializable_context = context.to_serializable()?;

        // Get the config with the entry point
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing configuration"))?;

        // Call the plugin function with the JSON data
        let output = plugin
            .call::<Json<serde_json::Value>, Json<HashMap<String, serde_json::Value>>>(
                &config.entry_point,
                json!(serializable_context.inputs).into(),
            )?;

        // Convert output bytes to string
        let output_value = output
            .into_inner()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone().into()))
            .collect();

        Ok(output_value)
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
    use reflow_network::actor::{MemoryState, Port};
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
        let wasm_binary = include_bytes!("../../../examples/wasm_actor/build/wasm_actor.wasm");

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
            "operation".to_string(),
            Message::String("increment".to_string()),
        );

        let context = crate::context::ScriptContext::new(
            config.entry_point,
            inputs,
            state.clone(),
            outports.clone(),
        );

        let result = engine.call(&context).await?;

        // Verify the result
        assert!(
            result.contains_key("value"),
            "Result should contain 'value' key"
        );
        assert_eq!(result["value"], Message::Integer(1));
        assert!(
            result.contains_key("previous"),
            "Result should contain 'previous' key"
        );
        assert_eq!(result["previous"], Message::Integer(0));
        assert!(
            result.contains_key("operation"),
            "Result should contain 'operation' key"
        );
        assert_eq!(
            result["operation"],
            Message::String("increment".to_string())
        );

        if let Ok(msg) = outports.1.recv() {
            // Verify the message
            assert_eq!(msg.len(), 3);
            assert!(msg.contains_key("value"));
            assert_eq!(msg["value"], Message::Integer(1));
            assert!(msg.contains_key("previous"));
            assert_eq!(msg["previous"], Message::Integer(0));
            assert!(msg.contains_key("operation"));
            assert_eq!(msg["operation"], Message::String("increment".to_string()));
        }

        // Print state 
        let state_guard = state.lock();
        assert_eq!(state_guard.0, HashMap::from_iter([("counter".to_string(), json!(1))]));

        engine.cleanup().await?;
        Ok(())
    }
}
