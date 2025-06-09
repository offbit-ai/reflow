//! Error Handling Components
//!
//! This module contains components that manage error detection, reporting, and recovery.

use std::{collections::HashMap, sync::Arc};

use actor_macro::actor;
use anyhow::Error;
use parking_lot::Mutex;
use reflow_network::actor::ActorContext;

use crate::{Actor, ActorBehavior, ActorLoad, MemoryState, Message, Port};

/// Handles errors from operations.
///
/// # Inports
/// - `In`: Operation input
/// - `Operation`: Operation to perform (as string identifier)
///
/// # Outports
/// - `Success`: Result of successful operation
/// - `Error`: Error information if operation failed
#[actor(
    TryCatchActor,
    inports::<100>(In, Operation),
    outports::<50>(Success, Error),
    state(MemoryState)
)]
async fn try_catch_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let input = match payload.get("In") {
        Some(msg) => msg,
        None => return Ok([].into()), // No input, no output
    };

    // Get operation to perform
    let operation = payload
        .get("Operation")
        .and_then(|m| match m {
            Message::String(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| {
            let state = state.lock();
            if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
                state_data
                    .get("operation")
                    .and_then(|v| v.as_str())
                    .unwrap_or("identity")
                    .to_string()
            } else {
                "identity".to_string()
            }
        });

    // Attempt the operation
    let result = match operation.as_str() {
        "identity" => Ok(input.clone()),
        "parse_json" => {
            if let Message::String(s) = input {
                match serde_json::from_str::<serde_json::Value>(s) {
                    Ok(v) => Ok(v.into()),
                    Err(e) => Err(format!("JSON parse error: {}", e)),
                }
            } else {
                Err("Input is not a string".to_string())
            }
        }
        "parse_int" => {
            if let Message::String(s) = input {
                match s.parse::<i64>() {
                    Ok(i) => Ok(Message::Integer(i)),
                    Err(e) => Err(format!("Integer parse error: {}", e)),
                }
            } else {
                Err("Input is not a string".to_string())
            }
        }
        "parse_float" => {
            if let Message::String(s) = input {
                match s.parse::<f64>() {
                    Ok(f) => Ok(Message::Float(f)),
                    Err(e) => Err(format!("Float parse error: {}", e)),
                }
            } else {
                Err("Input is not a string".to_string())
            }
        }
        "divide" => {
            // Get divisor from state
            let divisor = {
                let state = state.lock();
                if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
                    state_data
                        .get("divisor")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(1.0)
                } else {
                    1.0
                }
            };

            if divisor == 0.0 {
                Err("Division by zero".to_string())
            } else {
                match input {
                    Message::Integer(i) => Ok(Message::Float(*i as f64 / divisor)),
                    Message::Float(f) => Ok(Message::Float(f / divisor)),
                    _ => Err("Input is not a number".to_string()),
                }
            }
        }
        "get_property" => {
            // Get property name from state
            let property = {
                let state = state.lock();
                if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
                    state_data
                        .get("property")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                } else {
                    "".to_string()
                }
            };

            if property.is_empty() {
                Err("No property specified".to_string())
            } else if let Message::Object(obj) = input {
                if let serde_json::Value::Object(map) = serde_json::to_value(obj).unwrap() {
                    if let Some(value) = map.get(&property) {
                        Ok(value.clone().into())
                    } else {
                        Err(format!("Property '{}' not found", property))
                    }
                } else {
                    Err("Failed to convert object".to_string())
                }
            } else {
                Err("Input is not an object".to_string())
            }
        }
        "array_index" => {
            // Get index from state
            let index = {
                let state = state.lock();
                if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
                    state_data
                        .get("index")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as usize
                } else {
                    0
                }
            };

            if let Message::Array(arr) = input {
                if let Some(value) = arr.get(index) {
                    Ok(serde_json::to_value(value.clone())?.into())
                } else {
                    Err(format!("Index {} out of bounds", index))
                }
            } else {
                Err("Input is not an array".to_string())
            }
        }
        "custom" => {
            // Get custom operation function from state
            let custom_op = {
                let state = state.lock();
                if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
                    state_data
                        .get("custom_operation")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                } else {
                    "".to_string()
                }
            };

            if custom_op.is_empty() {
                Err("No custom operation defined".to_string())
            } else {
                // In a real implementation, this would evaluate the custom operation
                // For now, just return the input
                Ok(input.clone())
            }
        }
        _ => Err(format!("Unknown operation: {}", operation)),
    };

    // Route result to appropriate output port
    match result {
        Ok(value) => Ok([("Success".to_owned(), value)].into()),
        Err(error_msg) => Ok([("Error".to_owned(), Message::Error(error_msg))].into()),
    }
}

/// Validates input against a schema or condition.
///
/// # Inports
/// - `In`: Data to validate
/// - `Schema`: Validation schema or condition
///
/// # Outports
/// - `Valid`: Output when validation passes
/// - `Invalid`: Output when validation fails with error details
#[actor(
    ValidateActor,
    inports::<100>(In, Schema),
    outports::<50>(Valid, Invalid),
    state(MemoryState)
)]
async fn validate_actor(
   context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let input = match payload.get("In") {
        Some(msg) => msg,
        None => return Ok([].into()), // No input, no output
    };

    // Get validation schema/type from payload or state
    let validation_type = if let Some(Message::String(s)) = payload.get("Schema") {
        s.clone()
    } else {
        let state = state.lock();
        if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
            state_data
                .get("validation_type")
                .and_then(|v| v.as_str())
                .unwrap_or("type")
                .to_string()
        } else {
            "type".to_string()
        }
    };

    // Perform validation based on type
    let validation_result = match validation_type.as_str() {
        "type" => {
            // Get expected type from state
            let expected_type = {
                let state = state.lock();
                if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
                    state_data
                        .get("expected_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("any")
                        .to_string()
                } else {
                    "any".to_string()
                }
            };

            let actual_type = match input {
                Message::Flow => "flow",
                Message::Event(_) => "event",
                Message::Boolean(_) => "boolean",
                Message::Integer(_) => "integer",
                Message::Float(_) => "float",
                Message::String(_) => "string",
                Message::Object(_) => "object",
                Message::Array(_) => "array",
                Message::Stream(_) => "stream",
                Message::Encoded(_) => "encoded",
                Message::Optional(_) => "optional",
                Message::Any(_) => "any",
                Message::Error(_) => "error",
            };

            if expected_type == "any" || expected_type == actual_type {
                Ok(())
            } else {
                Err(format!(
                    "Expected type '{}', got '{}'",
                    expected_type, actual_type
                ))
            }
        }
        "range" => {
            // Get min and max from state
            let (min, max) = {
                let state = state.lock();
                if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
                    let min = state_data
                        .get("min")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(f64::NEG_INFINITY);
                    let max = state_data
                        .get("max")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(f64::INFINITY);
                    (min, max)
                } else {
                    (f64::NEG_INFINITY, f64::INFINITY)
                }
            };

            let value = match input {
                Message::Integer(i) => *i as f64,
                Message::Float(f) => *f,
                _ => {
                    return Ok([(
                        "Invalid".to_owned(),
                        Message::Error("Input is not a number".to_string()),
                    )]
                    .into())
                }
            };

            if value >= min && value <= max {
                Ok(())
            } else {
                Err(format!(
                    "Value {} is outside range [{}, {}]",
                    value, min, max
                ))
            }
        }
        "regex" => {
            // Get pattern from state
            let pattern = {
                let state = state.lock();
                if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
                    state_data
                        .get("pattern")
                        .and_then(|v| v.as_str())
                        .unwrap_or(".*")
                        .to_string()
                } else {
                    ".*".to_string()
                }
            };

            if let Message::String(s) = input {
                // In a real implementation, this would use a regex library
                // For now, just check if the pattern is contained in the string
                if pattern == ".*" || s.contains(&pattern) {
                    Ok(())
                } else {
                    Err(format!("String does not match pattern '{}'", pattern))
                }
            } else {
                Err("Input is not a string".to_string())
            }
        }
        "custom" => {
            // Get custom validation function from state
            let custom_validation = {
                let state = state.lock();
                if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
                    state_data
                        .get("custom_validation")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                } else {
                    "".to_string()
                }
            };

            if custom_validation.is_empty() {
                Err("No custom validation defined".to_string())
            } else {
                // In a real implementation, this would evaluate the custom validation
                // For now, just return success
                Ok(())
            }
        }
        _ => Err(format!("Unknown validation type: {}", validation_type)),
    };

    // Route result to appropriate output port
    match validation_result {
        Ok(_) => Ok([("Valid".to_owned(), input.clone())].into()),
        Err(error_msg) => Ok([("Invalid".to_owned(), Message::Error(error_msg))].into()),
    }
}
