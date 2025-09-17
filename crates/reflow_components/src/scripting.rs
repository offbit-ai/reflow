//! Zeal Scripting Actors
//!
//! Actors for executing scripts in various languages from Zeal templates.

use std::collections::HashMap;
use actor_macro::actor;
use anyhow::{Result, Error};
use reflow_actor::{ActorContext, message::EncodableValue};
use serde_json::{json, Value};
use crate::{Actor, ActorBehavior, ActorLoad, MemoryState, Message, Port};
use std::sync::Arc;
use reflow_actor::ActorConfig;
use reflow_tracing_protocol::client::TracingIntegration;
use rquickjs::{Runtime, Context as JsContext, Function as JsFunction};

/// JavaScript Script Actor - Compatible with tpl_javascript_script
///
/// Executes JavaScript code from Zeal node configuration.
#[actor(
    JavaScriptScriptActor,
    inports::<100>(input),
    outports::<50>(output, error),
    state(MemoryState)
)]
pub async fn javascript_script_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    let config = context.get_config_hashmap();
    let payload = context.get_payload();
    
    // Get input data
    let input = payload.get("input");
    
    // Get script from propertyValues (user-provided values)
    let property_values = config.get("propertyValues")
        .and_then(|v| v.as_object());
    
    let script = property_values
        .and_then(|pv| pv.get("script"))
        .or_else(|| config.get("script"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("No script provided"))?;
    
    // Create JavaScript runtime
    let runtime = Runtime::new()?;
    let ctx = JsContext::full(&runtime)?;
    
    // Execute within context
    ctx.with(|ctx| {
        // Set up global input variable if provided
        if let Some(input_data) = input {
            let input_json = serde_json::to_value(input_data)?;
            let globals = ctx.globals();
            
            // Convert JSON to JS value
            let js_input: rquickjs::Value = ctx.json_parse(input_json.to_string())?;
            globals.set("input", js_input)?;
        }
        
        // Execute the script
        match ctx.eval::<rquickjs::Value, _>(script) {
            Ok(js_result) => {
                // Convert JS value back to JSON
                let json_str = if let Some(s) = ctx.json_stringify(js_result)? {
                    s.to_string()?
                } else {
                    "null".to_string()
                };
                
                let output_value: Value = serde_json::from_str(&json_str)?;
                result.insert("output".to_string(), json_value_to_message(output_value));
            }
            Err(e) => {
                result.insert("error".to_string(), Message::Error(
                    format!("Script execution error: {}", e).into()
                ));
            }
        }
        
        Ok::<_, Error>(())
    })?;
    
    Ok(result)
}

/// SQL Script Actor - Compatible with tpl_sql_script
///
/// Prepares SQL queries from Zeal node configuration.
#[actor(
    SqlScriptActor,
    inports::<100>(parameters),
    outports::<50>(query, error),
    state(MemoryState)
)]
pub async fn sql_script_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    let config = context.get_config_hashmap();
    let payload = context.get_payload();
    
    // Get parameters if any
    let params = payload.get("parameters");
    
    // Get SQL script from propertyValues
    let property_values = config.get("propertyValues")
        .and_then(|v| v.as_object());
    
    let sql = property_values
        .and_then(|pv| pv.get("script"))
        .or_else(|| config.get("script"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("No SQL script provided"))?;
    
    // Simple parameter substitution (in production, use proper parameterized queries)
    let mut processed_sql = sql.to_string();
    
    if let Some(Message::Object(params_obj)) = params {
        let params_json = serde_json::to_value(params_obj)?;
        if let Value::Object(map) = params_json {
            for (key, value) in map {
                let placeholder = format!(":{}", key);
                let replacement = match value {
                    Value::String(s) => format!("'{}'", s.replace("'", "''")),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => "NULL".to_string(),
                    _ => value.to_string()
                };
                processed_sql = processed_sql.replace(&placeholder, &replacement);
            }
        }
    }
    
    result.insert("query".to_string(), Message::String(processed_sql.into()));
    Ok(result)
}

/// Python Script Actor - Compatible with tpl_python_script
///
/// This actor wraps reflow_script's ScriptActor for Python execution.
/// For now, we'll provide a placeholder that shows how to configure it.
#[actor(
    PythonScriptActor,
    inports::<100>(input),
    outports::<50>(output, error),
    state(MemoryState)
)]
pub async fn python_script_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    let config = context.get_config_hashmap();
    let payload = context.get_payload();
    
    // Get script from propertyValues
    let property_values = config.get("propertyValues")
        .and_then(|v| v.as_object());
    
    let script = property_values
        .and_then(|pv| pv.get("script"))
        .or_else(|| config.get("script"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("No Python script provided"))?;
    
    // To use reflow_script's ScriptActor, we would need to:
    // 1. Create a ScriptConfig with ScriptRuntime::Python
    // 2. Create a ScriptActor instance
    // 3. Execute it with the context
    
    // For now, return a placeholder that indicates Python script configuration
    result.insert("output".to_string(), Message::object(EncodableValue::from(json!({
        "message": "Python script execution requires reflow_script::ScriptActor",
        "script_length": script.len(),
        "note": "Configure ScriptActor with ScriptRuntime::Python for actual execution"
    }))));
    
    Ok(result)
}

/// Evaluate JavaScript expression with given context data
pub fn evaluate_js_expression(expression: &str, context_data: &Value) -> Result<Value> {
    let runtime = Runtime::new()?;
    let ctx = JsContext::full(&runtime)?;
    
    ctx.with(|ctx| {
        // Set up context data as global variables
        if let Value::Object(map) = context_data {
            let globals = ctx.globals();
            for (key, value) in map {
                let js_value: rquickjs::Value = ctx.json_parse(value.to_string())?;
                globals.set(key.as_str(), js_value)?;
            }
        }
        
        // Evaluate the expression
        let js_result: rquickjs::Value = ctx.eval(expression)?;
        
        // Convert result back to JSON
        let json_str = if let Some(s) = ctx.json_stringify(js_result)? {
            s.to_string()?
        } else {
            "null".to_string()
        };
        
        let result: Value = serde_json::from_str(&json_str)?;
        Ok(result)
    })
}

/// Evaluate JavaScript filter expression
pub fn evaluate_js_filter(expression: &str, item: &Value) -> Result<bool> {
    let runtime = Runtime::new()?;
    let ctx = JsContext::full(&runtime)?;
    
    ctx.with(|ctx| {
        // Create a function that takes the item as parameter
        let filter_fn = format!("(function(item) {{ return {}; }})", expression);
        
        // Compile the function
        let func: JsFunction = ctx.eval(filter_fn)?;
        
        // Convert item to JS value
        let js_item: rquickjs::Value = ctx.json_parse(item.to_string())?;
        
        // Call the function
        let result: bool = func.call((js_item,))?;
        
        Ok(result)
    })
}

/// Evaluate JavaScript transform expression
pub fn evaluate_js_transform(expression: &str, data: &Value) -> Result<Value> {
    let runtime = Runtime::new()?;
    let ctx = JsContext::full(&runtime)?;
    
    ctx.with(|ctx| {
        // Create a function that takes the data as parameter
        let transform_fn = format!("(function(data) {{ return {}; }})", expression);
        
        // Compile the function
        let func: JsFunction = ctx.eval(transform_fn)?;
        
        // Convert data to JS value
        let js_data: rquickjs::Value = ctx.json_parse(data.to_string())?;
        
        // Call the function
        let js_result: rquickjs::Value = func.call((js_data,))?;
        
        // Convert result back to JSON
        let json_str = if let Some(s) = ctx.json_stringify(js_result)? {
            s.to_string()?
        } else {
            "null".to_string()
        };
        
        let result: Value = serde_json::from_str(&json_str)?;
        Ok(result)
    })
}

// Helper function to convert JSON value to Message
fn json_value_to_message(value: Value) -> Message {
    match value {
        Value::Null => Message::Optional(None),
        Value::Bool(b) => Message::Boolean(b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Message::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Message::Float(f)
            } else {
                Message::Float(0.0)
            }
        }
        Value::String(s) => Message::String(s.into()),
        Value::Array(arr) => {
            let items: Vec<EncodableValue> = arr.into_iter()
                .map(|v| v.into())
                .collect();
            Message::Array(items.into())
        }
        Value::Object(_) => Message::object(EncodableValue::from(value))
    }
}