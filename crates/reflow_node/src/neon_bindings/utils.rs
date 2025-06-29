//! Utility functions for Neon bindings
//! 
//! Provides helper functions for converting between JS and Rust types

use neon::prelude::*;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Get an optional string argument from function context
pub fn get_optional_string_arg(cx: &mut FunctionContext, index: usize) -> Option<String> {
    cx.argument_opt(index)
        .and_then(|v| v.downcast::<JsString, _>(cx).ok())
        .map(|s| s.value(cx))
}

/// Get an optional boolean argument from function context
pub fn get_optional_bool_arg(cx: &mut FunctionContext, index: usize) -> Option<bool> {
    cx.argument_opt(index)
        .and_then(|v| v.downcast::<JsBoolean, _>(cx).ok())
        .map(|b| b.value(cx))
}

/// Get an optional number argument from function context
pub fn get_optional_number_arg(cx: &mut FunctionContext, index: usize) -> Option<f64> {
    cx.argument_opt(index)
        .and_then(|v| v.downcast::<JsNumber, _>(cx).ok())
        .map(|n| n.value(cx))
}

/// Convert JavaScript value to serde_json::Value
pub fn js_to_json_value<'cx>(cx: &mut Cx<'cx>, value: Handle<JsValue>) -> NeonResult<JsonValue> {
    if value.is_a::<JsNull, _>(cx) || value.is_a::<JsUndefined, _>(cx) {
        Ok(JsonValue::Null)
    } else if let Ok(b) = value.downcast::<JsBoolean, _>(cx) {
        Ok(JsonValue::Bool(b.value(cx)))
    } else if let Ok(n) = value.downcast::<JsNumber, _>(cx) {
        let num = n.value(cx);
        if num.fract() == 0.0 && num >= i64::MIN as f64 && num <= i64::MAX as f64 {
            Ok(JsonValue::Number(serde_json::Number::from(num as i64)))
        } else {
            Ok(JsonValue::Number(
                serde_json::Number::from_f64(num)
                    .unwrap_or(serde_json::Number::from(0))
            ))
        }
    } else if let Ok(s) = value.downcast::<JsString, _>(cx) {
        Ok(JsonValue::String(s.value(cx)))
    } else if let Ok(arr) = value.downcast::<JsArray, _>(cx) {
        let mut vec = Vec::new();
        for i in 0..arr.len(cx) {
            let item = arr.get::<JsValue, _, _>(cx, i)?;
            vec.push(js_to_json_value(cx, item)?);
        }
        Ok(JsonValue::Array(vec))
    } else if let Ok(obj) = value.downcast::<JsObject, _>(cx) {
        let prop_names = obj.get_own_property_names(cx)?;
        let mut map = serde_json::Map::new();
        
        for i in 0..prop_names.len(cx) {
            let key_val = prop_names.get::<JsValue, _, _>(cx, i)?;
            if let Ok(key_str) = key_val.downcast::<JsString, _>(cx) {
                let key = key_str.value(cx);
                let prop_val = obj.get::<JsValue, _, _>(cx, key_str)?;
                map.insert(key, js_to_json_value(cx, prop_val)?);
            }
        }
        Ok(JsonValue::Object(map))
    } else {
        Ok(JsonValue::Null)
    }
}

/// Convert serde_json::Value to JavaScript value
pub fn json_value_to_js<'a>(cx: &mut Cx<'a>, value: &JsonValue) -> NeonResult<Handle<'a, JsValue>> {
    match value {
        JsonValue::Null => Ok(cx.null().upcast()),
        JsonValue::Bool(b) => Ok(cx.boolean(*b).upcast()),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(cx.number(i as f64).upcast())
            } else if let Some(f) = n.as_f64() {
                Ok(cx.number(f).upcast())
            } else {
                Ok(cx.number(0.0).upcast())
            }
        }
        JsonValue::String(s) => Ok(cx.string(s).upcast()),
        JsonValue::Array(arr) => {
            let js_arr = JsArray::new(cx, arr.len());
            for (i, item) in arr.iter().enumerate() {
                match item {
                    JsonValue::Null => {
                        let null_val = cx.null();
                        js_arr.set(cx, i as u32, null_val)?;
                    }
                    JsonValue::Bool(b) => {
                        let bool_val = cx.boolean(*b);
                        js_arr.set(cx, i as u32, bool_val)?;
                    }
                    JsonValue::Number(n) => {
                        let num_val = if let Some(i) = n.as_i64() {
                            cx.number(i as f64)
                        } else if let Some(f) = n.as_f64() {
                            cx.number(f)
                        } else {
                            cx.number(0.0)
                        };
                        js_arr.set(cx, i as u32, num_val)?;
                    }
                    JsonValue::String(s) => {
                        let str_val = cx.string(s);
                        js_arr.set(cx, i as u32, str_val)?;
                    }
                    _ => {
                        // For nested objects/arrays, use recursive call
                        let js_item = json_value_to_js(cx, item)?;
                        js_arr.set(cx, i as u32, js_item)?;
                    }
                }
            }
            Ok(js_arr.upcast())
        }
        JsonValue::Object(obj) => {
            let js_obj = JsObject::new(cx);
            for (key, val) in obj.iter() {
                match val {
                    JsonValue::Null => {
                        let key_handle = cx.string(key);
                        let null_val = cx.null();
                        js_obj.set(cx, key_handle, null_val)?;
                    }
                    JsonValue::Bool(b) => {
                        let key_handle = cx.string(key);
                        let bool_val = cx.boolean(*b);
                        js_obj.set(cx, key_handle, bool_val)?;
                    }
                    JsonValue::Number(n) => {
                        let key_handle = cx.string(key);
                        let num_val = if let Some(i) = n.as_i64() {
                            cx.number(i as f64)
                        } else if let Some(f) = n.as_f64() {
                            cx.number(f)
                        } else {
                            cx.number(0.0)
                        };
                        js_obj.set(cx, key_handle, num_val)?;
                    }
                    JsonValue::String(s) => {
                        let key_handle = cx.string(key);
                        let str_val = cx.string(s);
                        js_obj.set(cx, key_handle, str_val)?;
                    }
                    _ => {
                        // For nested objects/arrays, use recursive call
                        let js_val = json_value_to_js(cx, val)?;
                        let key_handle = cx.string(key);
                        js_obj.set(cx, key_handle, js_val)?;
                    }
                }
            }
            Ok(js_obj.upcast())
        }
    }
}

/// Convert JS array to Vec<String>
pub fn js_array_to_string_vec(cx: &mut FunctionContext, arr: Handle<JsArray>) -> NeonResult<Vec<String>> {
    let mut vec = Vec::new();
    for i in 0..arr.len(cx) {
        if let Ok(item) = arr.get::<JsString, _, _>(cx, i) {
            vec.push(item.value(cx));
        }
    }
    Ok(vec)
}

/// Convert Vec<String> to JS array
pub fn string_vec_to_js_array<'a>(cx: &mut FunctionContext<'a>, vec: &[String]) -> NeonResult<Handle<'a, JsArray>> {
    let js_arr = JsArray::new(cx, vec.len());
    for (i, item) in vec.iter().enumerate() {
        let js_str = cx.string(item);
        js_arr.set(cx, i as u32, js_str)?;
    }
    Ok(js_arr)
}

/// Convert JS object to HashMap<String, JsonValue>
pub fn js_object_to_map<'cx>(cx: &mut Cx<'cx>, obj: Handle<JsObject>) -> NeonResult<HashMap<String, JsonValue>> {
    let prop_names = obj.get_own_property_names(cx)?;
    let mut map = HashMap::new();
    
    for i in 0..prop_names.len(cx) {
        let key_val = prop_names.get::<JsValue, _, _>(cx, i)?;
        if let Ok(key_str) = key_val.downcast::<JsString, _>(cx) {
            let key = key_str.value(cx);
            let prop_val = obj.get::<JsValue, _, _>(cx, key_str)?;
            map.insert(key, js_to_json_value(cx, prop_val)?);
        }
    }
    Ok(map)
}

/// Convert HashMap<String, JsonValue> to JS object
pub fn map_to_js_object<'a>(cx: &mut FunctionContext<'a>, map: &HashMap<String, JsonValue>) -> NeonResult<Handle<'a, JsObject>> {
    let js_obj = JsObject::new(cx);
    for (key, val) in map.iter() {
        match val {
            JsonValue::Null => {
                let key_handle = cx.string(key);
                let null_val = cx.null();
                js_obj.set(cx, key_handle, null_val)?;
            }
            JsonValue::Bool(b) => {
                let key_handle = cx.string(key);
                let bool_val = cx.boolean(*b);
                js_obj.set(cx, key_handle, bool_val)?;
            }
            JsonValue::Number(n) => {
                let key_handle = cx.string(key);
                let num_val = if let Some(i) = n.as_i64() {
                    cx.number(i as f64)
                } else if let Some(f) = n.as_f64() {
                    cx.number(f)
                } else {
                    cx.number(0.0)
                };
                js_obj.set(cx, key_handle, num_val)?;
            }
            JsonValue::String(s) => {
                let key_handle = cx.string(key);
                let str_val = cx.string(s);
                js_obj.set(cx, key_handle, str_val)?;
            }
            _ => {
                // For nested objects/arrays, use recursive call
                let js_val = json_value_to_js(cx, val)?;
                let key_handle = cx.string(key);
                js_obj.set(cx, key_handle, js_val)?;
            }
        }
    }
    Ok(js_obj)
}

/// Create a Rust error from a string message
pub fn create_error<'a>(cx: &mut FunctionContext<'a>, message: &str) -> NeonResult<Handle<'a, JsValue>> {
    let error = cx.error(message)?;
    Ok(error.upcast())
}

/// Throw a Rust error with a string message
pub fn throw_error<T>(cx: &mut FunctionContext, message: &str) -> NeonResult<T> {
    cx.throw_error(message)
}

/// Handle Result<T, E> where E: Display, converting errors to JS exceptions
pub fn handle_result<T, E: std::fmt::Display>(
    cx: &mut FunctionContext,
    result: Result<T, E>
) -> NeonResult<T> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => cx.throw_error(format!("{}", error)),
    }
}

/// Execute an async operation synchronously (blocking)
/// This is a simplified version for cases where we need to call async Rust code from sync JS
pub fn execute_sync_async<F, T, E>(operation: F) -> Result<T, E>
where
    F: std::future::Future<Output = Result<T, E>>,
{
    // Note: This uses a simple blocking approach
    // In a real implementation, you might want to use a proper async runtime
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(operation)
    })
}

/// Get the current timestamp as a string
pub fn get_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Generate a UUID string
pub fn generate_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Validate a string as a valid identifier (alphanumeric + underscore, starting with letter)
pub fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    
    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        if !first.is_alphabetic() && first != '_' {
            return false;
        }
        
        chars.all(|c| c.is_alphanumeric() || c == '_')
    } else {
        false
    }
}

/// Sanitize a string to be a valid identifier
pub fn sanitize_identifier(s: &str) -> String {
    if s.is_empty() {
        return "identifier".to_string();
    }
    
    let mut result = String::new();
    let mut chars = s.chars();
    
    // Handle first character
    if let Some(first) = chars.next() {
        if first.is_alphabetic() || first == '_' {
            result.push(first);
        } else if first.is_numeric() {
            result.push('_');
            result.push(first);
        } else {
            result.push('_');
        }
    }
    
    // Handle remaining characters
    for c in chars {
        if c.is_alphanumeric() || c == '_' {
            result.push(c);
        } else {
            result.push('_');
        }
    }
    
    if result.is_empty() {
        result = "identifier".to_string();
    }
    
    result
}
