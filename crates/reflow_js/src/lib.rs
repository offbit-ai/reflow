pub mod runtime;
pub mod minimal_runtime;
pub mod web;
pub mod filesystem;
pub mod resolver;
pub mod networking;
pub mod snapshot;
pub mod process;
pub mod minimal_test;
pub mod minimal_runtime_test;
pub mod phase1_test;
pub mod flume_function_test;

use anyhow::{Result, Error as AnyError};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use reflow_actor::message::Message;
use std::sync::Arc;
use std::collections::HashMap;
use flume::Sender;
use tokio::sync::RwLock;

pub use runtime::*;
pub use web::*;
pub use filesystem::*;
pub use resolver::*;
pub use networking::*;
pub use process::*;

/// Simplified JavaScript runtime interface - maintains backward compatibility
#[derive(Clone)]
pub struct JavascriptRuntime {
    pub worker: Arc<CoreRuntime>,
    callback_registry: Arc<RwLock<HashMap<String, CallbackData>>>,
}

/// Internal callback data storage
#[derive(Clone)]
struct CallbackData {
    sender: Option<Sender<HashMap<String, Message>>>,
    function_type: CallbackType,
}

#[derive(Clone)]
enum CallbackType {
    SendOutput,
    Generic,
}

impl JavascriptRuntime {
    /// Create a new JavaScript runtime with all extensions
    /// Note: This is a synchronous constructor that will spawn a new runtime if needed
    pub fn new() -> Result<Self, AnyError> {
        // Create a default config for backward compatibility
        let config = CoreRuntimeConfig {
            permissions: PermissionOptions {
                allow_all: true,
                ..Default::default()
            },
            typescript: true,
            unstable_apis: true,
            ..Default::default()
        };

        // Check if we're already in a tokio runtime context
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // We're in an async context, we need to spawn a blocking task
            let runtime = std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    CoreRuntimeFactory::create_runtime(config).await
                })
            }).join().map_err(|_| anyhow::anyhow!("Failed to create runtime in thread"))??;

            Ok(Self {
                worker: Arc::new(runtime),
                callback_registry: Arc::new(RwLock::new(HashMap::new())),
            })
        } else {
            // We're not in an async context, we can block directly
            let runtime = tokio::runtime::Runtime::new()?;
            let core_runtime = runtime.block_on(async {
                CoreRuntimeFactory::create_runtime(config).await
            })?;

            Ok(Self {
                worker: Arc::new(core_runtime),
                callback_registry: Arc::new(RwLock::new(HashMap::new())),
            })
        }
    }

    /// Async constructor for proper initialization
    pub async fn new_async(config: CoreRuntimeConfig) -> Result<Self, AnyError> {
        let runtime = CoreRuntimeFactory::create_runtime(config).await?;
        
        Ok(Self {
            worker: Arc::new(runtime),
            callback_registry: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Execute JavaScript/TypeScript code
    pub async fn execute(&mut self, name: &str, source: &str) -> Result<GlobalValue, AnyError> {
        let json_result = self.worker.execute(name, source).await?;
        Ok(GlobalValue::from_json(json_result, self.worker.clone()))
    }

    /// Convert a GlobalValue to a Rust type
    pub fn convert_value_to_rust<T>(&self, value: GlobalValue) -> T
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_value(value.json_value).expect("Could not convert value")
    }

    /// Convert a Rust value to GlobalValue
    pub fn convert_value_from_rust(&self, value: serde_json::Value) -> Result<GlobalValue, AnyError> {
        Ok(GlobalValue::from_json(value, self.worker.clone()))
    }

    /// Create a number value
    pub fn create_number(&mut self, number: f64) -> GlobalValue {
        let json_val = JsonValue::Number(serde_json::Number::from_f64(number).unwrap_or_else(|| serde_json::Number::from(0)));
        GlobalValue::from_json(json_val, self.worker.clone())
    }

    /// Create a string value
    pub fn create_string(&mut self, string: &str) -> GlobalValue {
        let json_val = JsonValue::String(string.to_string());
        GlobalValue::from_json(json_val, self.worker.clone())
    }

    /// Create an array value
    pub fn create_array(&mut self, array: &[GlobalValue]) -> GlobalValue {
        let json_array: Vec<JsonValue> = array.iter()
            .map(|gv| gv.json_value.clone())
            .collect();
        let json_val = JsonValue::Array(json_array);
        GlobalValue::from_json(json_val, self.worker.clone())
    }

    /// Create an object value
    pub fn create_object(&mut self) -> GlobalObject {
        GlobalObject::new(serde_json::Map::new(), self.worker.clone())
    }

    /// Set property on an object
    pub fn object_set_property(
        &mut self,
        mut object: GlobalObject,
        key: &str,
        value: GlobalValue,
    ) -> GlobalObject {
        object.properties.insert(key.to_string(), value.json_value);
        object
    }

    /// Convert object to value
    pub fn obj_to_value(&mut self, obj: GlobalObject) -> GlobalValue {
        let json_val = JsonValue::Object(obj.properties);
        GlobalValue::from_json(json_val, self.worker.clone())
    }

    /// Create a callback handle from a GlobalValue
    pub fn create_callback_handle(&self, _value: GlobalValue) -> CallbackHandle {
        CallbackHandle {
            inner: CallbackValue {
                function_id: Uuid::new_v4().to_string(),
                runtime_ref: self.worker.clone(),
                callback_type: CallbackType::Generic,
            }
        }
    }

    /// Factory method to create a send_output callback
    pub fn create_send_output_callback(
        &mut self,
        sender: Sender<HashMap<String, Message>>,
    ) -> Result<CallbackHandle, AnyError> {
        let function_id = Uuid::new_v4().to_string();
        
        // Store callback data
        let callback_data = CallbackData {
            sender: Some(sender),
            function_type: CallbackType::SendOutput,
        };
        
        // Register callback asynchronously
        let registry = self.callback_registry.clone();
        let id = function_id.clone();
        tokio::spawn(async move {
            let mut registry = registry.write().await;
            registry.insert(id, callback_data);
        });

        Ok(CallbackHandle {
            inner: CallbackValue {
                function_id,
                runtime_ref: self.worker.clone(),
                callback_type: CallbackType::SendOutput,
            }
        })
    }

    /// Set property on an object using a CallbackHandle
    pub fn object_set_property_with_callback(
        &mut self,
        mut object: GlobalObject,
        key: &str,
        callback: CallbackHandle,
    ) -> GlobalObject {
        let func_marker = JsonValue::Object({
            let mut map = serde_json::Map::new();
            map.insert("__function_id".to_string(), JsonValue::String(callback.inner.function_id));
            map.insert("__callback_type".to_string(), JsonValue::String(
                match callback.inner.callback_type {
                    CallbackType::SendOutput => "send_output".to_string(),
                    CallbackType::Generic => "generic".to_string(),
                }
            ));
            map
        });
        
        object.properties.insert(key.to_string(), func_marker);
        object
    }

    /// Register a Rust function in the JavaScript context
    pub async fn register_function<F>(&mut self, name: &str, func: F) -> Result<(), AnyError>
    where
        F: Fn(&[JsonValue]) -> Result<JsonValue> + Send + Sync + 'static,
    {
        self.worker.register_function(name, func).await
    }
}

/// Wrapper for JavaScript values - maintains API compatibility
#[derive(Clone)]
pub struct GlobalValue {
    json_value: JsonValue,
    runtime: Arc<CoreRuntime>,
}

impl GlobalValue {
    fn from_json(json: JsonValue, runtime: Arc<CoreRuntime>) -> Self {
        Self {
            json_value: json,
            runtime,
        }
    }

    /// Convert to JSON for serialization
    pub fn to_json(&self) -> JsonValue {
        self.json_value.clone()
    }

    /// Check if this is a promise (simplified detection)
    pub fn is_promise(&self) -> bool {
        // In a full implementation, this would check for Promise-like structure
        if let JsonValue::Object(obj) = &self.json_value {
            obj.contains_key("then") && obj.contains_key("catch")
        } else {
            false
        }
    }

    /// Wait for promise resolution (if this is a promise)
    pub async fn await_promise(self) -> Result<GlobalValue, AnyError> {
        // For now, just return self since we don't have full promise support yet
        // In full implementation, this would await the actual promise
        Ok(self)
    }
}

/// Wrapper for JavaScript objects - maintains API compatibility
#[derive(Clone)]
pub struct GlobalObject {
    properties: serde_json::Map<String, JsonValue>,
    runtime: Arc<CoreRuntime>,
}

impl GlobalObject {
    fn new(properties: serde_json::Map<String, JsonValue>, runtime: Arc<CoreRuntime>) -> Self {
        Self {
            properties,
            runtime,
        }
    }

    /// Get a property from the object
    pub fn get_property(&self, key: &str) -> Option<GlobalValue> {
        self.properties.get(key)
            .map(|value| GlobalValue::from_json(value.clone(), self.runtime.clone()))
    }

    /// Set a property on the object
    pub fn set_property(&mut self, key: &str, value: GlobalValue) {
        self.properties.insert(key.to_string(), value.json_value);
    }

    /// Check if object has a property
    pub fn has_property(&self, key: &str) -> bool {
        self.properties.contains_key(key)
    }

    /// Get all property names
    pub fn property_names(&self) -> Vec<String> {
        self.properties.keys().cloned().collect()
    }
}

/// Callback handle for JavaScript functions - maintains API compatibility
#[derive(Clone)]
pub struct CallbackHandle {
    pub(crate) inner: CallbackValue,
}

#[derive(Clone)]
struct CallbackValue {
    function_id: String,
    runtime_ref: Arc<CoreRuntime>,
    callback_type: CallbackType,
}

impl CallbackHandle {
    /// Get the function ID
    pub fn function_id(&self) -> &str {
        &self.inner.function_id
    }

    /// Call the callback with arguments (simplified)
    pub async fn call(&self, args: Vec<JsonValue>) -> Result<JsonValue, AnyError> {
        // In full implementation, this would invoke the actual JavaScript function
        // For now, return a placeholder result
        Ok(JsonValue::Object({
            let mut map = serde_json::Map::new();
            map.insert("called".to_string(), JsonValue::Bool(true));
            map.insert("args_count".to_string(), JsonValue::Number(serde_json::Number::from(args.len())));
            map
        }))
    }
}

// Convenience type aliases for backward compatibility
pub type v8GlobalValue = GlobalValue;
pub type v8GlobalObject = GlobalObject;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_javascript_runtime_creation() {
        let config = CoreRuntimeConfig::default();
        let runtime = JavascriptRuntime::new_async(config).await;
        assert!(runtime.is_ok());
    }
    
    #[tokio::test]
    async fn test_basic_execution() {
        let config = CoreRuntimeConfig::default();
        let mut runtime = JavascriptRuntime::new_async(config).await.unwrap();
        
        let result = runtime.execute("test", "1 + 1").await;
        assert!(result.is_ok());
        
        let value = result.unwrap();
        let json = value.to_json();
        
        if let JsonValue::Number(n) = json {
            assert_eq!(n.as_f64(), Some(2.0));
        } else {
            panic!("Expected number result, got: {:?}", json);
        }
    }
    
    #[tokio::test]
    async fn test_value_conversion() {
        let config = CoreRuntimeConfig::default();
        let mut runtime = JavascriptRuntime::new_async(config).await.unwrap();
        
        // Test number conversion
        let num_value = runtime.create_number(42.0);
        let converted: f64 = runtime.convert_value_to_rust(num_value);
        assert_eq!(converted, 42.0);
        
        // Test string conversion
        let str_value = runtime.create_string("hello");
        let converted: String = runtime.convert_value_to_rust(str_value);
        assert_eq!(converted, "hello");
    }
    
    #[tokio::test]
    async fn test_object_creation() {
        let config = CoreRuntimeConfig::default();
        let mut runtime = JavascriptRuntime::new_async(config).await.unwrap();
        
        let mut obj = runtime.create_object();
        let key_value = runtime.create_string("value");
        
        obj = runtime.object_set_property(obj, "key", key_value);
        
        assert!(obj.has_property("key"));
        
        let retrieved = obj.get_property("key").unwrap();
        let converted: String = runtime.convert_value_to_rust(retrieved);
        assert_eq!(converted, "value");
    }
    
    #[tokio::test]
    async fn test_array_creation() {
        let config = CoreRuntimeConfig::default();
        let mut runtime = JavascriptRuntime::new_async(config).await.unwrap();
        
        let values = vec![
            runtime.create_number(1.0),
            runtime.create_number(2.0),
            runtime.create_number(3.0),
        ];
        
        let array = runtime.create_array(&values);
        let json = array.to_json();
        
        if let JsonValue::Array(arr) = json {
            assert_eq!(arr.len(), 3);
            
            for (i, item) in arr.iter().enumerate() {
                if let JsonValue::Number(n) = item {
                    assert_eq!(n.as_f64(), Some((i + 1) as f64));
                } else {
                    panic!("Expected number in array");
                }
            }
        } else {
            panic!("Expected array result");
        }
    }
    
    #[tokio::test]
    async fn test_console_log_execution() {
        let config = CoreRuntimeConfig::default();
        let mut runtime = JavascriptRuntime::new_async(config).await.unwrap();
        
        let result = runtime.execute("test", "console.log('Hello, World!'); 'success'").await;
        assert!(result.is_ok());
        
        let value = result.unwrap();
        let json = value.to_json();
        
        if let JsonValue::String(s) = json {
            assert_eq!(s, "success");
        } else {
            panic!("Expected string result, got: {:?}", json);
        }
    }
}
