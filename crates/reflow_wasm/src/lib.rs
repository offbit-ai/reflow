use anyhow::Result;
pub use extism_pdk::{host_fn, FnResult, Json};

use std::collections::HashMap;

pub use extism_pdk::plugin_fn as actor_fn;


pub type Input = Json<HashMap<String, serde_json::Value>>;
pub type Output = Json<HashMap<String, serde_json::Value>>;

/// Context struct hosts methods for actor operations
/// Example:
/// ```rust
/// use reflow_wasm::Context;
/// let context = Context::new();
/// let result = context.get_state("key");
/// ```
pub struct Context {}

#[host_fn("extism:host/user")]
extern "ExtismHost" {
    fn __send_output(output: Json<HashMap<String, serde_json::Value>>) -> ();
    fn __get_state(key: String) -> Json<serde_json::Value>;
    fn __set_state(key: String, value: Json<serde_json::Value>) -> ();
}

impl Context {
    /// Create a new Context with the given context ID
    pub fn new() -> Self {
        Self {}
    }

    /// Send output to the specified ports
    pub fn send_output(&self, outputs: HashMap<String, serde_json::Value>) -> FnResult<()> {
        unsafe { __send_output(Json(outputs))? };
        Ok(())
    }

    /// Get a value from the state
    pub fn get_state(&self, key: &str) -> FnResult<serde_json::Value> {
        let result = unsafe { __get_state(key.to_string())? };
        Ok(result.0)
    }

    /// Set a value in the state
    pub fn set_state(&self, key: &str, value: serde_json::Value) -> FnResult<()> {
        unsafe { __set_state(key.to_string(), Json(value)) }?;
        Ok(())
    }
}




// Example usage:
//
// #[actor_fn]
// pub fn call(input: Json<Input>) -> FnResult<Json<HashMap<String, serde_json::Value>>> {
//     // Extract the input data
//     let input_data = input.0;
//     let context = Context::new(input_data.context_id);
//
//     // Process inputs
//     let value = input_data.inputs.get("value")
//         .and_then(|v| v.as_i64())
//         .unwrap_or(0);
//     let result = value * 2;
//
//     // Create output
//     let mut output = HashMap::new();
//     output.insert("out".to_string(), serde_json::json!(result));
//
//     Ok(Json(output))
// }
