use std::{any::Any, collections::HashMap, pin::Pin, rc::Rc, sync::Arc, env};

#[cfg(target_arch = "wasm32")]
use gloo_utils::format::JsValueSerdeExt;
use parking_lot::Mutex;
#[cfg(not(target_arch = "wasm32"))]
use rayon::ThreadPool;
use serde_json::Value;
#[cfg(target_arch = "wasm32")]
use std::fmt::Debug;
#[cfg(target_arch = "wasm32")]
use tsify::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::convert::FromWasmAbi;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::{message::Message, network::Network, graph::types::GraphNode};

// #[cfg(not(target_arch = "wasm32"))]
pub type ActorBehavior = Box<
    dyn Fn(
            ActorContext,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<HashMap<String, Message>, anyhow::Error>>
                    + Send
                    + 'static,
            >,
        > + Send
        + Sync
        + 'static,
>;

pub type ActorPayload = HashMap<String, Message>;
pub type ActorChannel = (
    flume::Sender<crate::message::Message>,
    flume::Receiver<crate::message::Message>,
);

// #[cfg(not(target_arch = "wasm32"))]
pub type Port = (
    flume::Sender<HashMap<String, crate::message::Message>>,
    flume::Receiver<HashMap<String, crate::message::Message>>,
);

/// Enhanced configuration for actors containing metadata, environment variables, and resolved config
#[derive(Debug, Clone)]
pub struct ActorConfig {
    /// Full GraphNode snapshot including metadata
    pub node: GraphNode,
    /// Resolved environment variables
    pub resolved_env: HashMap<String, String>,
    /// Final processed configuration combining metadata and environment variables
    pub config: HashMap<String, Value>,
    /// Graph namespace (for multi-graph support)
    pub namespace: Option<String>,
}

impl Default for ActorConfig {
    fn default() -> Self {
        Self {
            node: GraphNode {
                id: "default".to_string(),
                component: "DefaultComponent".to_string(),
                metadata: Some(HashMap::new()),
            },
            resolved_env: HashMap::new(),
            config: HashMap::new(),
            namespace: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingRequiredEnvVar(String),
    #[error("Invalid environment variable value for {0}: {1}")]
    InvalidEnvValue(String, String),
    #[error("Configuration parsing error: {0}")]
    ParseError(String),
}

impl ActorConfig {
    /// Create ActorConfig from a GraphNode, resolving environment variables
    pub fn from_node(node: GraphNode) -> Result<Self, ConfigError> {
        Self::from_node_with_namespace(node, None)
    }

    /// Create ActorConfig from a GraphNode with namespace support
    pub fn from_node_with_namespace(node: GraphNode, namespace: Option<String>) -> Result<Self, ConfigError> {
        let mut resolved_env = HashMap::new();
        let mut config = HashMap::new();

        // Start with metadata as base config
        if let Some(metadata) = &node.metadata {
            config.extend(metadata.clone());
        }

        // Process environment variable requirements
        if let Some(metadata) = &node.metadata {
            if let Some(env_vars) = metadata.get("env_vars") {
                if let Some(env_vars_obj) = env_vars.as_object() {
                    for (env_key, requirement) in env_vars_obj {
                        let requirement_str = requirement.as_str().unwrap_or("required");
                        
                        match Self::resolve_env_var(env_key, requirement_str)? {
                            Some(value) => {
                                resolved_env.insert(env_key.clone(), value.clone());
                                // Also add to config for backwards compatibility
                                config.insert(env_key.clone(), Value::String(value));
                            }
                            None => {
                                // Optional environment variable not found - that's OK
                            }
                        }
                    }
                }
            }
        }

        Ok(ActorConfig {
            node,
            resolved_env,
            config,
            namespace,
        })
    }

    /// Resolve a single environment variable
    fn resolve_env_var(env_key: &str, requirement: &str) -> Result<Option<String>, ConfigError> {
        match env::var(env_key) {
            Ok(value) => Ok(Some(value)),
            Err(env::VarError::NotPresent) => {
                if requirement.starts_with("required") {
                    Err(ConfigError::MissingRequiredEnvVar(env_key.to_string()))
                } else if let Some(default) = requirement.strip_prefix("optional:") {
                    Ok(Some(default.to_string()))
                } else {
                    Ok(None)
                }
            }
            Err(env::VarError::NotUnicode(_)) => {
                Err(ConfigError::InvalidEnvValue(
                    env_key.to_string(),
                    "Invalid UTF-8".to_string(),
                ))
            }
        }
    }

    /// Get environment variable value
    pub fn get_env(&self, key: &str) -> Option<&String> {
        self.resolved_env.get(key)
    }

    /// Get metadata from the original GraphNode
    pub fn get_metadata(&self) -> Option<&HashMap<String, Value>> {
        self.node.metadata.as_ref()
    }

    /// Get component name from the GraphNode
    pub fn get_component(&self) -> &str {
        &self.node.component
    }

    /// Get node ID from the GraphNode
    pub fn get_node_id(&self) -> &str {
        &self.node.id
    }

    /// Helper method to get string value from config
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.config.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
    }

    /// Helper method to get number value from config
    pub fn get_number(&self, key: &str) -> Option<f64> {
        self.config.get(key).and_then(|v| v.as_f64())
    }

    /// Helper method to get boolean value from config
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.config.get(key).and_then(|v| v.as_bool())
    }

    /// Helper method to get integer value from config
    pub fn get_integer(&self, key: &str) -> Option<i64> {
        self.config.get(key).and_then(|v| v.as_i64())
    }

    /// Get the full config as HashMap for backwards compatibility
    pub fn as_hashmap(&self) -> HashMap<String, Value> {
        self.config.clone()
    }

    /// Get a config value with environment variable fallback
    pub fn get_config_or_env(&self, key: &str) -> Option<String> {
        // Try config first, then environment variable
        self.get_string(key).or_else(|| self.get_env(key).cloned())
    }
}

// #[cfg(not(target_arch = "wasm32"))]
pub trait Actor: Send + Sync + 'static {
    /// Trait method to get actor's behavior
    fn get_behavior(&self) -> ActorBehavior;
    /// Access all output ports
    fn get_outports(&self) -> Port;
    /// Access all input ports
    fn get_inports(&self) -> Port;

    fn load_count(&self) -> Arc<parking_lot::Mutex<ActorLoad>> {
        Arc::new(Mutex::new(ActorLoad::new(0)))
    }

    fn create_process(
        &self,
        config: ActorConfig,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>>;

    /// Shutdown the actor, waiting for all processes to finish
    fn shutdown(&self) {
        while self.load_count().clone().lock().get() > 0 {
            // Wait for all processes to finish
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    fn cleanup(&self) {
        // Should be implemented by the actor to clean up resources
    }
}



// Native ActorLoad for non-WASM targets (tuple struct)
#[cfg(not(target_arch = "wasm32"))]
pub struct ActorLoad(pub usize);

#[cfg(not(target_arch = "wasm32"))]
impl ActorLoad {
    pub fn new(load: usize) -> Self {
        ActorLoad(load)
    }

    pub fn inc(&mut self) {
        self.0 += 1;
    }

    pub fn dec(&mut self) {
        if self.0 > 0 {
            self.0 -= 1;
        }
    }

    pub fn get(&self) -> usize {
        self.0
    }

    pub fn reset(&mut self) {
        self.0 = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

// WASM-specific ActorLoad (regular struct)
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
#[wasm_bindgen]
pub struct ActorLoad {
    value: usize,
}

#[cfg(target_arch = "wasm32")]
impl ActorLoad {
    pub fn new(load: usize) -> Self {
        ActorLoad { value: load }
    }

    pub fn inc(&mut self) {
        self.value += 1;
    }

    pub fn dec(&mut self) {
        if self.value > 0 {
            self.value -= 1;
        }
    }

    pub fn get(&self) -> usize {
        self.value
    }

    pub fn reset(&mut self) {
        self.value = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.value == 0
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl ActorLoad {
    #[wasm_bindgen(constructor)]
    pub fn new_js(load: usize) -> ActorLoad {
        ActorLoad::new(load)
    }

    #[wasm_bindgen(js_name = increment)]
    pub fn increment(&mut self) {
        self.inc()
    }

    #[wasm_bindgen(js_name = decrement)]
    pub fn decrement(&mut self) {
        self.dec()
    }

    #[wasm_bindgen(js_name = getValue)]
    pub fn get_value(&self) -> usize {
        self.get()
    }

    #[wasm_bindgen(js_name = reset)]
    pub fn reset_load(&mut self) {
        self.reset()
    }

    #[wasm_bindgen(js_name = isEmpty)]
    pub fn is_empty_load(&self) -> bool {
        self.is_empty()
    }
}

pub struct ActorContext {
    // pub id: String,
    pub payload: ActorPayload,
    pub outports: Port,
    pub state: Arc<Mutex<dyn ActorState>>,
    pub config: ActorConfig,
    load: Arc<Mutex<ActorLoad>>,
}

impl ActorContext {
    pub fn new(
        // id: String,
        payload: ActorPayload,
        outports: Port,
        state: Arc<Mutex<dyn ActorState>>,
        config: ActorConfig,
        load: Arc<Mutex<ActorLoad>>,
    ) -> Self {
        ActorContext {
            // id,
            payload,
            outports,
            state,
            config,
            load,
        }
    }


    pub fn get_state(&self) -> Arc<Mutex<dyn ActorState>> {
        self.state.clone()
    }

    pub fn get_config(&self) -> &ActorConfig {
        &self.config
    }

    /// Get config as HashMap for backwards compatibility
    pub fn get_config_hashmap(&self) -> HashMap<String, Value> {
        self.config.as_hashmap()
    }

    pub fn get_load(&self) -> Arc<Mutex<ActorLoad>> {
        self.load.clone()
    }
    // pub fn get_id(&self) -> &str {
    //     &self.id
    // }
    pub fn get_payload(&self) -> &ActorPayload {
        &self.payload
    }
    pub fn get_outports(&self) -> Port {
        self.outports.clone()
    }
    pub fn done(&self) {
        self.load.lock().reset();
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct WasmActorContext {
    context: ActorContext,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl WasmActorContext {
    #[wasm_bindgen(constructor)]
    pub fn new(payload: JsValue, config: JsValue) -> Result<WasmActorContext, JsValue> {
        let payload_map = payload
            .into_serde::<HashMap<String, Value>>()
            .map_err(|e| JsValue::from_str(&format!("Failed to parse payload: {}", e)))?
            .into_iter()
            .map(|(k, v)| (k, Message::from(v)))
            .collect();

        let config_map = config
            .into_serde::<HashMap<String, Value>>()
            .map_err(|e| JsValue::from_str(&format!("Failed to parse config: {}", e)))?;

        let outports = flume::unbounded();
        let state = Arc::new(Mutex::new(MemoryState::default()));
        let load = Arc::new(Mutex::new(ActorLoad::new(0)));

        // Create ActorConfig from HashMap for backwards compatibility
        let node = GraphNode {
            id: "wasm_actor".to_string(),
            component: "WasmComponent".to_string(),
            metadata: Some(config_map.clone()),
        };

        let actor_config = ActorConfig {
            node,
            resolved_env: HashMap::new(),
            config: config_map,
            namespace: None,
        };

        Ok(WasmActorContext {
            context: ActorContext::new(payload_map, outports, state, actor_config, load),
        })
    }

    #[wasm_bindgen(js_name = getPayload)]
    pub fn get_payload(&self) -> JsValue {
        let payload_map = self
            .context
            .get_payload()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone().into()))
            .collect::<HashMap<String, Value>>();

        JsValue::from_serde(&payload_map).unwrap_or(JsValue::NULL)
    }

    #[wasm_bindgen(js_name = getConfig)]
    pub fn get_config(&self) -> JsValue {
        JsValue::from_serde(self.context.get_config()).unwrap_or(JsValue::NULL)
    }

    #[wasm_bindgen(js_name = getState)]
    pub fn get_state(&self) -> Option<MemoryState> {
        if let Some(state) = self.context.get_state().try_lock() {
            if let Some(memory_state) = state.as_any().downcast_ref::<MemoryState>() {
                return Some(memory_state.clone());
            }
        }
        None
    }

    #[wasm_bindgen(js_name = setState)]
    pub fn set_state(&self, state: MemoryState) -> Result<(), JsValue> {
        if let Some(mut current_state) = self.context.get_state().try_lock() {
            if let Some(memory_state) = current_state.as_mut_any().downcast_mut::<MemoryState>() {
                *memory_state = state;
                return Ok(());
            }
        }
        Err(JsValue::from_str("Failed to set state"))
    }

    #[wasm_bindgen(js_name = sendToOutport)]
    pub fn send_to_outport(&self, port_name: &str, data: JsValue) -> Result<(), JsValue> {
        if let Ok(value) = data.into_serde::<Value>() {
            let mut messages = HashMap::new();
            messages.insert(port_name.to_string(), Message::from(value));

            self.context
                .outports
                .0
                .send(messages)
                .map_err(|e| JsValue::from_str(&format!("Failed to send message: {}", e)))?;

            Ok(())
        } else {
            Err(JsValue::from_str("Failed to parse message data"))
        }
    }

    #[wasm_bindgen(js_name = done)]
    pub fn done(&self) {
        self.context.done();
    }
}

pub trait ActorState: Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
    fn as_mut_any(&mut self) -> &mut dyn Any;
}

// Native MemoryState for non-WASM targets
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default, Debug, Clone)]
pub struct MemoryState(pub HashMap<String, Value>);

#[cfg(not(target_arch = "wasm32"))]
impl ActorState for MemoryState {
    fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }

    fn as_mut_any(&mut self) -> &mut dyn Any {
        self as &mut dyn Any
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl MemoryState {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.0.get_mut(key)
    }

    pub fn insert(&mut self, key: &str, value: Value) {
        self.0.insert(key.to_string(), value);
    }

    pub fn has_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }
    pub fn remove(&mut self, key: &str) {
        self.0.remove(key);
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

// WASM-specific MemoryState using HashMap (Send + Sync safe)
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Default)]
#[wasm_bindgen]
pub struct MemoryState {
    data: HashMap<String, Value>,
}

#[cfg(target_arch = "wasm32")]
impl ActorState for MemoryState {
    fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }

    fn as_mut_any(&mut self) -> &mut dyn Any {
        self as &mut dyn Any
    }
}

// Implement Send and Sync manually for WASM MemoryState
#[cfg(target_arch = "wasm32")]
unsafe impl Send for MemoryState {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for MemoryState {}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl MemoryState {
    #[wasm_bindgen(constructor)]
    pub fn new() -> MemoryState {
        MemoryState::default()
    }

    #[wasm_bindgen(js_name = get)]
    pub fn get(&self, key: &str) -> JsValue {
        self.data
            .get(key)
            .map(|v| JsValue::from_serde(v).unwrap_or(JsValue::NULL))
            .unwrap_or(JsValue::UNDEFINED)
    }

    #[wasm_bindgen(js_name = set)]
    pub fn set(&mut self, key: &str, value: JsValue) -> Result<(), JsValue> {
        if let Ok(val) = value.into_serde::<Value>() {
            self.data.insert(key.to_string(), val);
            Ok(())
        } else {
            Err(JsValue::from_str("Failed to convert value"))
        }
    }

    #[wasm_bindgen(js_name = has)]
    pub fn has_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    #[wasm_bindgen(js_name = remove)]
    pub fn remove(&mut self, key: &str) -> bool {
        self.data.remove(key).is_some()
    }

    #[wasm_bindgen(js_name = clear)]
    pub fn clear(&mut self) {
        self.data.clear();
    }

    #[wasm_bindgen(js_name = size)]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[wasm_bindgen(js_name = getAll)]
    pub fn get_object(&self) -> JsValue {
        JsValue::from_serde(&self.data).unwrap_or(JsValue::NULL)
    }

    #[wasm_bindgen(js_name = setAll)]
    pub fn set_object(&mut self, state: JsValue) {
        if let Ok(map) = state.into_serde::<HashMap<String, Value>>() {
            self.data = map;
        }
    }

    #[wasm_bindgen(js_name = keys)]
    pub fn keys(&self) -> js_sys::Array {
        let keys = js_sys::Array::new();
        for key in self.data.keys() {
            keys.push(&JsValue::from_str(key));
        }
        keys
    }
}

#[cfg(target_arch = "wasm32")]
impl MemoryState {
    // Internal methods for Rust code
    pub fn get_value(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    pub fn insert(&mut self, key: &str, value: Value) {
        self.data.insert(key.to_string(), value);
    }

    pub fn get_hashmap(&self) -> HashMap<String, Value> {
        self.data.clone()
    }

    pub fn set_hashmap(&mut self, map: HashMap<String, Value>) {
        self.data = map;
    }
}

// LiveMemoryState - implements ActorState and provides the actual shared state
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Default)]
pub struct LiveMemoryState {
    data: HashMap<String, Value>,
}

#[cfg(target_arch = "wasm32")]
impl ActorState for LiveMemoryState {
    fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }

    fn as_mut_any(&mut self) -> &mut dyn Any {
        self as &mut dyn Any
    }
}

#[cfg(target_arch = "wasm32")]
unsafe impl Send for LiveMemoryState {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for LiveMemoryState {}

#[cfg(target_arch = "wasm32")]
impl LiveMemoryState {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn get_value(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    pub fn insert(&mut self, key: &str, value: Value) {
        self.data.insert(key.to_string(), value);
    }

    pub fn get_hashmap(&self) -> HashMap<String, Value> {
        self.data.clone()
    }

    pub fn set_hashmap(&mut self, map: HashMap<String, Value>) {
        self.data = map;
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.data.remove(key)
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn keys(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }
}

// LiveMemoryStateHandle - WASM bindings for JavaScript access to shared state
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct LiveMemoryStateHandle {
    state_ref: Arc<Mutex<LiveMemoryState>>,
}

#[cfg(target_arch = "wasm32")]
impl LiveMemoryStateHandle {
    pub fn new(state_ref: Arc<Mutex<LiveMemoryState>>) -> Self {
        Self { state_ref }
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl LiveMemoryStateHandle {
    #[wasm_bindgen(js_name = get)]
    pub fn get(&self, key: &str) -> JsValue {
        let state = self.state_ref.lock();
        state
            .get_value(key)
            .map(|v| JsValue::from_serde(v).unwrap_or(JsValue::NULL))
            .unwrap_or(JsValue::UNDEFINED)
    }

    #[wasm_bindgen(js_name = set)]
    pub fn set(&self, key: &str, value: JsValue) -> Result<(), JsValue> {
        let mut state = self.state_ref.lock();
        if let Ok(val) = value.into_serde::<Value>() {
            state.insert(key, val);
            Ok(())
        } else {
            Err(JsValue::from_str("Failed to convert value"))
        }
    }

    #[wasm_bindgen(js_name = has)]
    pub fn has_key(&self, key: &str) -> bool {
        let state = self.state_ref.lock();
        state.contains_key(key)
    }

    #[wasm_bindgen(js_name = remove)]
    pub fn remove(&self, key: &str) -> bool {
        let mut state = self.state_ref.lock();
        state.remove(key).is_some()
    }

    #[wasm_bindgen(js_name = clear)]
    pub fn clear(&self) {
        let mut state = self.state_ref.lock();
        state.clear();
    }

    #[wasm_bindgen(js_name = size)]
    pub fn len(&self) -> usize {
        let state = self.state_ref.lock();
        state.len()
    }

    #[wasm_bindgen(js_name = getAll)]
    pub fn get_all(&self) -> JsValue {
        let state = self.state_ref.lock();
        JsValue::from_serde(&state.get_hashmap()).unwrap_or(JsValue::NULL)
    }

    #[wasm_bindgen(js_name = setAll)]
    pub fn set_all(&self, state_obj: JsValue) -> Result<(), JsValue> {
        let mut state = self.state_ref.lock();
        if let Ok(map) = state_obj.into_serde::<HashMap<String, Value>>() {
            state.set_hashmap(map);
            Ok(())
        } else {
            Err(JsValue::from_str("Failed to convert state object"))
        }
    }

    #[wasm_bindgen(js_name = keys)]
    pub fn keys(&self) -> js_sys::Array {
        let keys = js_sys::Array::new();
        let state = self.state_ref.lock();
        for key in state.keys() {
            keys.push(&JsValue::from_str(&key));
        }
        keys
    }
}

#[cfg(target_arch = "wasm32")]
impl Clone for LiveMemoryStateHandle {
    fn clone(&self) -> Self {
        Self {
            state_ref: self.state_ref.clone(),
        }
    }
}

// ActorRunContext - Unified context object for JavaScript actors
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct ActorRunContext {
    input: JsValue,
    state_handle: LiveMemoryStateHandle,
    config: JsValue,
    outports: Port,
}

#[cfg(target_arch = "wasm32")]
impl ActorRunContext {
    pub fn new(
        input: JsValue,
        state_handle: LiveMemoryStateHandle,
        config: HashMap<String, Value>,
        outports: Port,
    ) -> Self {
        let config_js = JsValue::from_serde(&config).unwrap_or(JsValue::NULL);
        Self {
            input,
            state_handle,
            config: config_js,
            outports,
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl ActorRunContext {
    #[wasm_bindgen(getter)]
    pub fn input(&self) -> JsValue {
        self.input.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn state(&self) -> LiveMemoryStateHandle {
        self.state_handle.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn config(&self) -> JsValue {
        self.config.clone()
    }

    #[wasm_bindgen(js_name = send)]
    pub fn send(&self, messages: JsValue) -> Result<(), JsValue> {
        let messages_map = messages
            .into_serde::<HashMap<String, serde_json::Value>>()
            .map_err(|e| JsValue::from_str(&format!("Failed to parse messages: {}", e)))?;

        let messages = messages_map
            .iter()
            .map(|(port, val)| (port.to_owned(), Message::from(val.clone())))
            .collect::<HashMap<String, Message>>();

        Network::send_outport_msg(self.outports.clone(), messages)
            .map_err(|e| JsValue::from_str(&format!("Failed to send messages: {}", e)))?;

        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_class = Actor)]
    pub type ExternActor;

    #[wasm_bindgen(method, getter)]
    pub fn inports(this: &ExternActor) -> Vec<String>;

    #[wasm_bindgen(method, getter)]
    pub fn outports(this: &ExternActor) -> Vec<String>;

    #[wasm_bindgen(method, getter)]
    pub fn state(this: &ExternActor) -> JsValue;

    #[wasm_bindgen(method, setter)]
    pub fn set_state(this: &ExternActor, state: LiveMemoryStateHandle);

    #[wasm_bindgen(method, getter, structural)]
    pub fn config(this: &ExternActor) -> JsValue;

    #[wasm_bindgen(method, structural)]
    pub fn run(this: &ExternActor, context: ActorRunContext);

}

trait WasmActorState: ActorState {
    fn get_object(&self) -> HashMap<String, Value>;
    fn set_object(&mut self, state: HashMap<String, Value>);
}

#[cfg(not(target_arch = "wasm32"))]
impl WasmActorState for MemoryState {
    fn get_object(&self) -> HashMap<String, Value> {
        self.0.clone()
    }

    fn set_object(&mut self, state: HashMap<String, Value>) {
        self.0 = state;
    }
}

#[cfg(target_arch = "wasm32")]
impl WasmActorState for MemoryState {
    fn get_object(&self) -> HashMap<String, Value> {
        self.get_hashmap()
    }

    fn set_object(&mut self, state: HashMap<String, Value>) {
        self.set_hashmap(state);
    }
}

#[cfg(target_arch = "wasm32")]
pub struct WasmActor {
    inports: Port,
    outports: Port,
    inports_size: usize,
    outports_size: usize,
    load: Arc<Mutex<ActorLoad>>,
    state: Arc<Mutex<dyn ActorState>>,
    behavior: Arc<ActorBehavior>,
    config: HashMap<String, Value>,
    extern_actor: ExternActor,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct JsWasmActor {
    actor: Arc<WasmActor>,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl JsWasmActor {
    #[wasm_bindgen(constructor)]
    pub fn new(extern_actor: ExternActor) -> Self {
        JsWasmActor {
            actor: Arc::new(WasmActor::new(extern_actor)),
        }
    }

    #[wasm_bindgen(js_name = getInportNames)]
    pub fn get_inport_names(&self) -> Vec<String> {
        self.actor.extern_actor.inports()
    }

    #[wasm_bindgen(js_name = getOutportNames)]
    pub fn get_outport_names(&self) -> Vec<String> {
        self.actor.extern_actor.outports()
    }

    #[wasm_bindgen(js_name = getState)]
    pub fn get_state(&self) -> JsValue {
        if let Some(state) = self.actor.state.try_lock() {
            if let Some(memory_state) = state.as_any().downcast_ref::<MemoryState>() {
                return memory_state.get_object();
            }
        }
        JsValue::NULL
    }

    #[wasm_bindgen(js_name = setState)]
    pub fn set_state(&self, state: JsValue) -> Result<(), JsValue> {
        if let Some(mut current_state) = self.actor.state.try_lock() {
            if let Some(memory_state) = current_state.as_mut_any().downcast_mut::<MemoryState>() {
                memory_state.set_object(state);
                return Ok(());
            }
        }
        Err(JsValue::from_str("Failed to set state"))
    }

    #[wasm_bindgen(js_name = getConfig)]
    pub fn get_config(&self) -> JsValue {
        JsValue::from_serde(&self.actor.config).unwrap_or(JsValue::NULL)
    }

    #[wasm_bindgen(js_name = sendMessage)]
    pub fn send_message(&self, port_name: &str, data: JsValue) -> Result<(), JsValue> {
        if let Ok(value) = data.into_serde::<Value>() {
            let mut messages = HashMap::new();
            messages.insert(port_name.to_string(), Message::from(value));

            self.actor
                .inports
                .0
                .send(messages)
                .map_err(|e| JsValue::from_str(&format!("Failed to send message: {}", e)))?;

            Ok(())
        } else {
            Err(JsValue::from_str("Failed to parse message data"))
        }
    }

    #[wasm_bindgen(js_name = getLoad)]
    pub fn get_load(&self) -> usize {
        self.actor.load.lock().get()
    }
}

#[cfg(target_arch = "wasm32")]
impl Actor for JsWasmActor {
    fn get_behavior(&self) -> ActorBehavior {
        // Clone the Arc to get a new reference to the behavior
        let behavior = self.actor.behavior.clone();
        Box::new(move |context| {
            let behavior_clone = behavior.clone();
            behavior_clone(context)
        })
    }

    fn get_outports(&self) -> Port {
        self.actor.outports.clone()
    }

    fn get_inports(&self) -> Port {
        self.actor.inports.clone()
    }

    fn create_process(
        &self,
        config: ActorConfig,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>> {
        self.actor.create_process(config)
    }
}

#[cfg(target_arch = "wasm32")]
impl WasmActor {
    pub fn new(extern_actor: ExternActor) -> Self {
        use serde_json::json;

        let inports = flume::unbounded();
        let outports = flume::unbounded();

        // Create shared LiveMemoryState that implements ActorState
        let shared_state = Arc::new(Mutex::new(LiveMemoryState::new()));

        // Initialize state from extern_actor if available
        if extern_actor.state().is_object() {
            if let Ok(state_map) = extern_actor.state().into_serde::<HashMap<String, Value>>() {
                let mut state = shared_state.lock();
                state.set_hashmap(state_map);
            }
        }

        // Create the live state handle for JavaScript access - this is the SAME reference used everywhere
        let state_handle = LiveMemoryStateHandle::new(shared_state.clone());

        // Inject the live state into the JavaScript actor
        extern_actor.set_state(state_handle.clone());

        let actor = extern_actor.clone();
        let load = Arc::new(Mutex::new(ActorLoad::new(0)));
        let config = extern_actor
            .config()
            .into_serde::<HashMap<String, Value>>()
            .unwrap_or_default();
        let shared_state_for_behavior = shared_state.clone();

        Self {
            inports,
            outports,
            inports_size: extern_actor.inports().len(),
            outports_size: extern_actor.outports().len(),
            load: load.clone(),
            state: shared_state, // Arc<Mutex<dyn ActorState>> - LiveMemoryState implements ActorState
            config: config.clone(),
            extern_actor: extern_actor.clone(),
            behavior: Arc::new(Box::new(move |context: ActorContext| {
                let actor_clone = actor.clone();
                let config_clone = config.clone();
                let shared_state_clone = shared_state_for_behavior.clone();
                Box::pin(async move {
                    let payload = context.payload.clone();
                    let outport_channels = context.outports.clone();

                    
                    // Convert payload to JsValue for input
                    let inputs = match JsValue::from_serde(&HashMap::<String, Value>::from_iter(payload.iter().map(|(k, v)| (k.to_string(), v.clone().into())))) {
                        Ok(val) => val,
                        Err(_) => return Err(anyhow::Error::msg("Failed to serialize payload")),
                    };

                    // Use the SAME shared state reference - this ensures true two-way binding!
                    let live_state_handle = LiveMemoryStateHandle::new(shared_state_clone);

                    // Create the unified context
                    let run_context = ActorRunContext::new(
                        inputs,
                        live_state_handle,
                        config_clone,
                        outport_channels,
                    );

                    // Call the JavaScript actor with the unified context
                    actor_clone.run(run_context);

                    // State is automatically synchronized through the shared Arc<Mutex<LiveMemoryState>>
                    // No manual synchronization needed!

                    // Decrement load counter when done
                    // context.done();

                    Ok(HashMap::new())
                })
            })),
        }
    }

    fn get_config(&self) -> HashMap<String, Value> {
        self.config.clone()
    }

    fn get_state(&self) -> Arc<Mutex<dyn ActorState>> {
        self.state.clone()
    }

    fn load_count(&self) -> Arc<Mutex<ActorLoad>> {
        self.load.clone()
    }
}

#[cfg(target_arch = "wasm32")]
impl Actor for WasmActor {
    fn get_behavior(&self) -> ActorBehavior {
        // Clone the Arc to get a new reference to the behavior
        let behavior = self.behavior.clone();
        Box::new(move |context| {
            let behavior_clone = behavior.clone();
            behavior_clone(context)
        })
    }

    fn get_outports(&self) -> Port {
        self.outports.clone()
    }

    fn get_inports(&self) -> Port {
        self.inports.clone()
    }

    fn create_process(
        &self,
        actor_config: ActorConfig,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>> {
        use futures::StreamExt;
        use serde_json::json;

        let outports = self.outports.clone();
        let behavior = self.get_behavior();
        let actor_state = self.get_state();
        let load = self.load_count();

        let inports_size = self.inports_size;

        let (_, receiver) = self.inports.clone();

        let await_all_inports = actor_config.config
            .get("await_all_inports")
            .unwrap_or(&json!(false))
            .as_bool()
            .unwrap();

        Box::pin(async move {
            let mut all_inports = std::collections::HashMap::new();
            loop {
                if let Some(packet) = receiver.clone().stream().next().await {
                    // Increment load counter
                    load.lock().inc();

                    if await_all_inports {
                        if all_inports.keys().len() < inports_size {
                            all_inports.extend(packet.iter().map(|(k, v)| (k.clone(), v.clone())));
                            if all_inports.keys().len() == inports_size {
                                // Run the behavior function
                                let context = ActorContext::new(
                                    all_inports.clone(),
                                    outports.clone(),
                                    actor_state.clone(),
                                    actor_config.clone(),
                                    load.clone(),
                                );

                                if let Ok(result) = behavior(context).await {
                                    if !result.is_empty() {
                                        let _ = outports
                                            .0
                                            .send(result)
                                            .expect("Expected to send message via outport");
                                        load.lock().dec();
                                    }
                                }
                            }
                            continue;
                        }
                    }

                    if !await_all_inports {
                        // Run the behavior function
                        let context = ActorContext::new(
                            packet,
                            outports.clone(),
                            actor_state.clone(),
                            actor_config.clone(),
                            load.clone(),
                        );

                        if let Ok(result) = behavior(context).await {
                            if !result.is_empty() {
                                let _ = outports
                                    .0
                                    .send(result)
                                    .expect("Expected to send message via outport");
                                load.lock().reset();
                            }
                        }
                    }
                }
            }
        })
    }
}

#[cfg(target_arch = "wasm32")]
impl Clone for ExternActor {
    fn clone(&self) -> Self {
        Self {
            obj: self.obj.clone(),
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl Debug for ExternActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternActor")
            .field("obj", &self.obj)
            .finish()
    }
}
#[cfg(target_arch = "wasm32")]
unsafe impl Send for ExternActor {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for ExternActor {}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
interface Actor {
    inports: Array<string>;
    outports: Array<string>;
    run(context: ActorRunContext): void;
    get state(): LiveMemoryStateHandle;
    set state(value: LiveMemoryStateHandle): void;
}

interface ActorRunContext {
    readonly input: any;
    readonly state: LiveMemoryStateHandle;
    readonly config: any;
    send(messages: any): void;
}

interface LiveMemoryStateHandle {
    get(key: string): any;
    set(key: string, value: any): void;
    has(key: string): boolean;
    remove(key: string): boolean;
    clear(): void;
    size(): number;
    getAll(): any;
    setAll(state: any): void;
    keys(): Array<string>;
}
"#;
