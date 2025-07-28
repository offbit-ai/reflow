//! Reflow WebAssembly Plugin SDK
//! 
//! This SDK enables developers to build Reflow actors as WebAssembly plugins using Extism PDK.
//! 
//! # Quick Start
//! 
//! ```rust,no_run
//! use reflow_wasm::*;
//! use anyhow::Result;
//! use std::collections::HashMap;
//! 
//! // Define your plugin metadata
//! fn metadata() -> PluginMetadata {
//!     PluginMetadata {
//!         component: "MyActor".to_string(),
//!         description: "Example actor plugin".to_string(),
//!         inports: vec![
//!             port_def!("input", "Main input port", "Integer", required),
//!         ],
//!         outports: vec![
//!             port_def!("output", "Main output port", "Integer"),
//!         ],
//!         config_schema: None,
//!     }
//! }
//! 
//! // Implement your actor behavior
//! fn process_actor(context: ActorContext) -> Result<ActorResult, Box<dyn std::error::Error>> {
//!     let mut outputs = HashMap::new();
//!     
//!     // Get input value
//!     if let Some(Message::Integer(value)) = context.payload.get("input") {
//!         // Process the value
//!         let result = value * 2;
//!         
//!         // Send to output
//!         outputs.insert("output".to_string(), Message::Integer(result));
//!     }
//!     
//!     Ok(ActorResult {
//!         outputs,
//!         state: None, // No state changes
//!     })
//! }
//! 
//! // Register the plugin
//! actor_plugin!(
//!     metadata: metadata(),
//!     process: process_actor
//! );
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export extism_pdk types
pub use extism_pdk::{plugin_fn, FnResult, Json};

// Message types that match reflow_actor::message::Message
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum Message {
    Flow,
    Event(serde_json::Value),
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Object(serde_json::Value),
    Array(Vec<serde_json::Value>),
    Stream(Vec<u8>),
    Optional(Option<Box<serde_json::Value>>),
    Any(serde_json::Value),
    Error(String),
}

impl From<serde_json::Value> for Message {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Message::Optional(None),
            serde_json::Value::Bool(b) => Message::Boolean(b),
            serde_json::Value::Number(n) => {
                if n.is_i64() {
                    Message::Integer(n.as_i64().unwrap())
                } else {
                    Message::Float(n.as_f64().unwrap())
                }
            }
            serde_json::Value::String(s) => Message::String(s),
            serde_json::Value::Array(vec) => Message::Array(vec),
            serde_json::Value::Object(_) => Message::Object(value),
        }
    }
}

impl Into<serde_json::Value> for Message {
    fn into(self) -> serde_json::Value {
        match self {
            Message::Flow => serde_json::Value::String("flow".to_string()),
            Message::Event(v) => v,
            Message::Boolean(b) => serde_json::Value::Bool(b),
            Message::Integer(i) => serde_json::Value::Number(i.into()),
            Message::Float(f) => serde_json::Value::Number(
                serde_json::Number::from_f64(f).unwrap_or(serde_json::Number::from(0))
            ),
            Message::String(s) => serde_json::Value::String(s),
            Message::Object(v) => v,
            Message::Array(arr) => serde_json::Value::Array(arr),
            Message::Stream(bytes) => serde_json::Value::Array(
                bytes.into_iter()
                    .map(|b| serde_json::Value::Number(b.into()))
                    .collect()
            ),
            Message::Optional(opt) => match opt {
                Some(v) => *v,
                None => serde_json::Value::Null,
            },
            Message::Any(v) => v,
            Message::Error(e) => serde_json::json!({
                "error": e
            }),
        }
    }
}

/// Actor configuration matching ActorConfig from reflow_actor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorConfig {
    /// Node ID from the graph
    pub node_id: String,
    /// Component name
    pub component: String,
    /// Resolved environment variables
    pub resolved_env: HashMap<String, String>,
    /// Final processed configuration
    pub config: HashMap<String, serde_json::Value>,
    /// Graph namespace (for multi-graph support)
    pub namespace: Option<String>,
}

impl ActorConfig {
    /// Get a string value from config
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.config.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
    
    /// Get a number value from config
    pub fn get_number(&self, key: &str) -> Option<f64> {
        self.config.get(key).and_then(|v| v.as_f64())
    }
    
    /// Get a boolean value from config
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.config.get(key).and_then(|v| v.as_bool())
    }
    
    /// Get an integer value from config
    pub fn get_integer(&self, key: &str) -> Option<i64> {
        self.config.get(key).and_then(|v| v.as_i64())
    }
    
    /// Get environment variable value
    pub fn get_env(&self, key: &str) -> Option<&String> {
        self.resolved_env.get(key)
    }
}

/// Context provided to actor behavior function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorContext {
    /// Input messages keyed by port name
    pub payload: HashMap<String, Message>,
    /// Actor configuration
    pub config: ActorConfig,
    /// Current state (as JSON for serialization)
    pub state: serde_json::Value,
}

/// Result of actor execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorResult {
    /// Output messages keyed by port name
    pub outputs: HashMap<String, Message>,
    /// Updated state (if changed)
    pub state: Option<serde_json::Value>,
}

/// Plugin metadata for actor discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Actor component name
    pub component: String,
    /// Human-readable description
    pub description: String,
    /// Input port definitions
    pub inports: Vec<PortDefinition>,
    /// Output port definitions
    pub outports: Vec<PortDefinition>,
    /// Configuration schema (JSON Schema)
    pub config_schema: Option<serde_json::Value>,
}

/// Port definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDefinition {
    /// Port name
    pub name: String,
    /// Port description
    pub description: String,
    /// Expected message type
    pub port_type: String,
    /// Whether the port is required
    pub required: bool,
}

/// State management trait for plugins
pub trait StateManager {
    fn get(&self, key: &str) -> Option<serde_json::Value>;
    fn set(&mut self, key: &str, value: serde_json::Value);
    fn get_all(&self) -> serde_json::Value;
    fn set_all(&mut self, state: serde_json::Value);
}

/// Default state manager implementation
#[derive(Debug, Clone, Default)]
pub struct MemoryState {
    data: HashMap<String, serde_json::Value>,
}

impl StateManager for MemoryState {
    fn get(&self, key: &str) -> Option<serde_json::Value> {
        self.data.get(key).cloned()
    }
    
    fn set(&mut self, key: &str, value: serde_json::Value) {
        self.data.insert(key.to_string(), value);
    }
    
    fn get_all(&self) -> serde_json::Value {
        serde_json::to_value(&self.data).unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
    }
    
    fn set_all(&mut self, state: serde_json::Value) {
        if let Ok(map) = serde_json::from_value::<HashMap<String, serde_json::Value>>(state) {
            self.data = map;
        }
    }
}

/// Macro to define an actor plugin
#[macro_export]
macro_rules! actor_plugin {
    (
        metadata: $metadata:expr,
        process: $process:expr
    ) => {
        pub use extism_pdk::{plugin_fn};
        use $crate::{Json, ActorContext, ActorResult, PluginMetadata, FnResult};
        
        /// Get plugin metadata
        #[plugin_fn]
        pub fn get_metadata() -> FnResult<Json<PluginMetadata>> {
            Ok(Json($metadata))
        }
        
        /// Process actor behavior
        #[plugin_fn]
        pub fn process(context: Json<ActorContext>) -> FnResult<Json<ActorResult>> {
            match $process(context.0) {
                Ok(result) => Ok(Json(result)),
                Err(e) => {
                    let err_msg = format!("Actor processing error: {}", e);
                    Err(extism_pdk::Error::msg(err_msg).into())
                }
            }
        }
    };
}

/// Macro to easily create port definitions
#[macro_export]
macro_rules! port_def {
    ($name:expr, $description:expr, $port_type:expr) => {
        $crate::PortDefinition {
            name: $name.to_string(),
            description: $description.to_string(),
            port_type: $port_type.to_string(),
            required: false,
        }
    };
    ($name:expr, $description:expr, $port_type:expr, required) => {
        $crate::PortDefinition {
            name: $name.to_string(),
            description: $description.to_string(),
            port_type: $port_type.to_string(),
            required: true,
        }
    };
}

/// Helper functions for common operations
pub mod helpers {
    use super::*;
    
    /// Create a simple transform actor
    pub fn create_transform_metadata(
        component: &str,
        description: &str,
        input_type: &str,
        output_type: &str,
    ) -> PluginMetadata {
        PluginMetadata {
            component: component.to_string(),
            description: description.to_string(),
            inports: vec![
                PortDefinition {
                    name: "input".to_string(),
                    description: "Input data".to_string(),
                    port_type: input_type.to_string(),
                    required: true,
                }
            ],
            outports: vec![
                PortDefinition {
                    name: "output".to_string(),
                    description: "Transformed output".to_string(),
                    port_type: output_type.to_string(),
                    required: false,
                }
            ],
            config_schema: None,
        }
    }
    
    /// Extract a value from the payload by port name
    pub fn get_input<T>(context: &ActorContext, port: &str) -> Option<T>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        context.payload.get(port)
            .and_then(|msg| {
                let value: serde_json::Value = msg.clone().into();
                serde_json::from_value(value).ok()
            })
    }
    
    /// Create an output message
    pub fn create_output(port: &str, value: impl Into<Message>) -> (String, Message) {
        (port.to_string(), value.into())
    }
    
    /// Create an error result
    pub fn error_result(error: impl std::fmt::Display) -> ActorResult {
        let mut outputs = HashMap::new();
        outputs.insert("error".to_string(), Message::Error(error.to_string()));
        ActorResult {
            outputs,
            state: None,
        }
    }
    
    /// Create a success result with single output
    pub fn success_result(port: &str, value: impl Into<Message>) -> ActorResult {
        let mut outputs = HashMap::new();
        outputs.insert(port.to_string(), value.into());
        ActorResult {
            outputs,
            state: None,
        }
    }
}

// Re-export helpers for convenience
pub use helpers::*;


/// Host function bindings for interacting with the Reflow runtime
pub mod host {
    use super::*;
    use extism_pdk::*;
    
    /// Send outputs asynchronously to the host
    /// This allows plugins to send messages without waiting for the process function to complete
    #[host_fn]
    extern "ExtismHost" {
        fn __send_output(outputs: Json<HashMap<String, serde_json::Value>>);
    }
    
    /// Get a value from the actor's state
    #[host_fn]
    extern "ExtismHost" {
        fn __get_state(key: &str) -> Json<serde_json::Value>;
    }
    
    /// Set a value in the actor's state
    #[host_fn]
    extern "ExtismHost" {
        fn __set_state(key: &str, value: Json<serde_json::Value>);
    }
    
    /// Send outputs asynchronously
    /// 
    /// # Example
    /// ```no_run
    /// use reflow_wasm::host::send_output;
    /// use std::collections::HashMap;
    /// 
    /// let mut outputs = HashMap::new();
    /// outputs.insert("status".to_string(), Message::String("processing".to_string()));
    /// send_output(outputs).unwrap();
    /// ```
    pub fn send_output(outputs: HashMap<String, Message>) -> Result<(), Error> {
        unsafe {
            // Convert HashMap<String, Message> to HashMap<String, serde_json::Value>
            // The host expects plain JSON values that can be converted using From<Value> for reflow_actor::Message
            let json_outputs: HashMap<String, serde_json::Value> = outputs
                .into_iter()
                .map(|(k, v)| {
                    // Convert WASM SDK Message to plain JSON Value that can be converted to reflow_actor::Message
                    let json_value = match v {
                        Message::Flow => serde_json::Value::String("flow".to_string()),
                        Message::Event(e) => e,
                        Message::Boolean(b) => serde_json::Value::Bool(b),
                        Message::Integer(i) => serde_json::Value::Number(i.into()),
                        Message::Float(f) => serde_json::Value::Number(serde_json::Number::from_f64(f).unwrap_or_else(|| 0.into())),
                        Message::String(s) => serde_json::Value::String(s),
                        Message::Object(o) => o,
                        Message::Array(a) => serde_json::Value::Array(a),
                        Message::Stream(s) => serde_json::Value::Array(s.into_iter().map(|b| serde_json::Value::Number(b.into())).collect()),
                        Message::Optional(o) => o.map(|boxed| *boxed).unwrap_or(serde_json::Value::Null),
                        Message::Any(a) => a,
                        Message::Error(e) => serde_json::Value::String(e),
                    };
                    (k, json_value)
                })
                .collect();
            
            __send_output(Json(json_outputs))?;
            Ok(())
        }
    }
    
    /// Get a value from the actor's state
    /// 
    /// # Example
    /// ```no_run
    /// use reflow_wasm::host::get_state;
    /// 
    /// let count: i64 = get_state("count")
    ///     .and_then(|v| v.as_i64())
    ///     .unwrap_or(0);
    /// ```
    pub fn get_state(key: &str) -> Option<serde_json::Value> {
        unsafe {
            match __get_state(key) {
                Ok(Json(value)) => {
                    if value.is_null() {
                        None
                    } else {
                        Some(value)
                    }
                },
                Err(_) => None,
            }
        }
    }
    
    /// Set a value in the actor's state
    /// 
    /// # Example
    /// ```no_run
    /// use reflow_wasm::host::set_state;
    /// 
    /// set_state("count", serde_json::json!(42)).unwrap();
    /// ```
    pub fn set_state(key: &str, value: serde_json::Value) -> Result<(), Error> {
        unsafe {
            __set_state(key, Json(value))?;
            Ok(())
        }
    }
}