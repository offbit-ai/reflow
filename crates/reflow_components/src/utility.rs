//! Utility Components
//!
//! This module contains general-purpose helper components.

use std::{collections::HashMap, sync::Arc};

use actor_macro::actor;
use anyhow::Error;
use parking_lot::Mutex;
use reflow_network::actor::ActorConfig;
use reflow_network::message::EncodableValue;

use crate::{Actor, ActorBehavior, ActorContext, ActorLoad, MemoryState, Message, Port};

/// Logs messages for debugging purposes.
///
/// # Inports
/// - `In`: Data to log
/// - `Level`: Log level (info, warn, error, debug)
///
/// # Outports
/// - `Out`: Passes through the input data
#[actor(
    LogActor,
    inports::<100>(In, Level),
    outports::<50>(Out),
    state(MemoryState)
)]
async fn log_actor(
    context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    if !payload.contains_key("In") {
        return Ok([].into()); // No input, no output
    }

    let input = payload.get("In").unwrap();

    // Get log level from payload or state
    let level = if let Some(Message::String(lvl)) = payload.get("Level") {
        lvl.clone()
    } else {
        let state = state.lock();
        if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
            state_data
                .get("level")
                .and_then(|v| v.as_str())
                .unwrap_or("info")
                .to_string().into()
        } else {
            "info".to_string().into()
        }
    };

    // Format the message
    let formatted = format!("{}: {:?}", level.to_uppercase(), input);

    // Log the message
    match level.to_lowercase().as_str() {
        "error" => eprintln!("[ERROR] {}", formatted),
        "warn" => eprintln!("[WARN] {}", formatted),
        "debug" => println!("[DEBUG] {}", formatted),
        _ => println!("[INFO] {}", formatted),
    }

    // Pass through the input
    Ok([("Out".to_owned(), input.clone())].into())
}

/// Delays execution for a specified time.
///
/// # Inports
/// - `In`: Data to pass through after delay
/// - `Delay`: Time to delay in milliseconds
///
/// # Outports
/// - `Out`: Delayed output
#[actor(
    DelayActor,
    inports::<100>(In, Delay),
    outports::<50>(Out),
    state(MemoryState)
)]
async fn delay_actor(
   context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let outport_channels = context.get_outports();
    if !payload.contains_key("In") {
        return Ok([].into()); // No input, no output
    }

    let input = payload.get("In").unwrap().clone();

    // Get delay in milliseconds
    let delay_ms = payload
        .get("Delay")
        .and_then(|m| match m {
            Message::Integer(i) => Some(*i),
            Message::Float(f) => Some(*f as i64),
            Message::String(s) => s.parse::<i64>().ok(),
            _ => None,
        })
        .unwrap_or(1000); // Default: 1 second

    // Store info in state
    {
        let mut state_lock = state.lock();
        if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
            state_data.insert("delay", serde_json::json!(delay_ms));
        }
    }

    // Spawn a delay task
    #[cfg(not(target_arch = "wasm32"))]
    {
        let outports = outport_channels.clone();

        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms as u64)).await;

            // Send message to output port
            let _ = outports
                .0
                .send_async([("Out".to_owned(), input)].into())
                .await;
        });
    }

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::prelude::*;

        let outports = outport_channels.clone();

        let timeout_callback = Closure::once(move || {
            // Send message to output port
            let _ = outports.0.send([("Out".to_owned(), input)].into());
        });

        let window = web_sys::window().expect("no global window exists");
        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                timeout_callback.as_ref().unchecked_ref(),
                delay_ms as i32,
            )
            .expect("Failed to set timeout");

        timeout_callback.forget(); // Prevent closure from being dropped
    }

    // Return empty result since output is sent asynchronously
    Ok([].into())
}

/// Generates a unique identifier.
///
/// # Inports
/// - `Trigger`: Signal to generate a new ID
/// - `Prefix`: Optional prefix for the ID
///
/// # Outports
/// - `ID`: Generated unique identifier
#[actor(
    UuidActor,
    inports::<100>(Trigger, Prefix),
    outports::<50>(ID),
    state(MemoryState)
)]
async fn uuid_actor(
   context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let outport_channels = context.get_outports();
    // Check if triggered
    if !payload.contains_key("Trigger") {
        return Ok([].into()); // No trigger, no output
    }

    // Get prefix if provided
    let prefix = if let Some(Message::String(p)) = payload.get("Prefix") {
        p.clone()
    } else {
        "".to_string().into()
    };

    // Generate a simple UUID (in a real implementation, use a proper UUID library)
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    let random = (timestamp % 10000) as u128;

    let uuid = if prefix.is_empty() {
        format!("{:x}-{:x}-{:x}", timestamp, random, timestamp / random)
    } else {
        format!(
            "{}-{:x}-{:x}-{:x}",
            prefix,
            timestamp,
            random,
            timestamp / random
        )
    };

    Ok([("ID".to_owned(), Message::string(uuid))].into())
}

/// Counts occurrences or accumulates a running total.
///
/// # Inports
/// - `In`: Increment trigger or value
/// - `Reset`: Signal to reset the counter
///
/// # Outports
/// - `Count`: Current count value
///
/// # State
/// - Maintains current count
#[actor(
    CounterActor,
    inports::<100>(In, Reset),
    outports::<50>(Count),
    state(MemoryState)
)]
async fn counter_actor(
   context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let outport_channels = context.get_outports();
    // Check for reset signal
    if payload.contains_key("Reset") {
        let reset = payload
            .get("Reset")
            .and_then(|m| match m {
                Message::Boolean(b) => Some(*b),
                Message::Integer(i) => Some(*i != 0),
                Message::String(s) => Some(!s.is_empty()),
                _ => Some(true), // Any non-boolean value triggers reset
            })
            .unwrap_or(true);

        if reset {
            // Reset counter
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                state_data.insert("count", serde_json::json!(0));
            }

            return Ok([("Count".to_owned(), Message::Integer(0))].into());
        }
    }

    // Check for increment
    if payload.contains_key("In") {
        let increment = match payload.get("In").unwrap() {
            Message::Integer(i) => *i,
            Message::Float(f) => *f as i64,
            _ => 1, // Default increment by 1 for non-numeric inputs
        };

        // Update counter
        let new_count = {
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                let current = state_data
                    .get("count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                let new_count = current + increment;
                state_data.insert("count", serde_json::json!(new_count));
                new_count
            } else {
                increment // No state, just return the increment
            }
        };

        return Ok([("Count".to_owned(), Message::Integer(new_count))].into());
    }

    // If neither reset nor increment, return current count
    let current = {
        let state_lock = state.lock();
        if let Some(state_data) = state_lock.as_any().downcast_ref::<MemoryState>() {
            state_data
                .get("count")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
        } else {
            0
        }
    };

    Ok([("Count".to_owned(), Message::Integer(current))].into())
}

/// Converts between different message types.
///
/// # Inports
/// - `In`: Input data to convert
/// - `Type`: Target type for conversion
///
/// # Outports
/// - `Out`: Converted data
/// - `Error`: Error information if conversion failed
#[actor(
    ConvertActor,
    inports::<100>(In, Type),
    outports::<50>(Out, Error),
    state(MemoryState)
)]
async fn convert_actor(
   context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    
    let input = match payload.get("In") {
        Some(msg) => msg,
        None => return Ok([].into()), // No input, no output
    };

    // Get target type from payload or state
    let target_type = if let Some(Message::String(t)) = payload.get("Type") {
        t.clone()
    } else {
        let state = state.lock();
        if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
            state_data
                .get("target_type")
                .and_then(|v| v.as_str())
                .unwrap_or("string")
                .to_string().into()
        } else {
            "string".to_string().into()
        }
    };

    // Attempt conversion
    match target_type.as_str() {
        "string" => {
            let result = match input {
                Message::Boolean(b) => Message::string(b.to_string()),
                Message::Integer(i) => Message::string(i.to_string()),
                Message::Float(f) => Message::string(f.to_string()),
                Message::String(_) => input.clone(),
                Message::Array(_) | Message::Object(_) => {
                    Message::string(serde_json::to_string(input).unwrap_or_default())
                }
                _ => {
                    return Ok([(
                        "Error".to_owned(),
                        Message::error(format!("Cannot convert {:?} to string", input)),
                    )]
                    .into());
                }
            };

            Ok([("Out".to_owned(), result)].into())
        }
        "boolean" => {
            let result = match input {
                Message::Boolean(_) => input.clone(),
                Message::Integer(i) => Message::Boolean(*i != 0),
                Message::Float(f) => Message::Boolean(*f != 0.0),
                Message::String(s) => {
                    let lower = s.to_lowercase();
                    if ["true", "yes", "1", "on"].contains(&lower.as_str()) {
                        Message::Boolean(true)
                    } else if ["false", "no", "0", "off"].contains(&lower.as_str()) {
                        Message::Boolean(false)
                    } else {
                        return Ok([(
                            "Error".to_owned(),
                            Message::error(format!("Cannot convert string '{}' to boolean", s)),
                        )]
                        .into());
                    }
                }
                _ => {
                    return Ok([(
                        "Error".to_owned(),
                        Message::error(format!("Cannot convert {:?} to boolean", input)),
                    )]
                    .into());
                }
            };

            Ok([("Out".to_owned(), result)].into())
        }
        "integer" => {
            let result = match input {
                Message::Boolean(b) => Message::Integer(if *b { 1 } else { 0 }),
                Message::Integer(_) => input.clone(),
                Message::Float(f) => Message::Integer(*f as i64),
                Message::String(s) => match s.parse::<i64>() {
                    Ok(i) => Message::Integer(i),
                    Err(_) => {
                        return Ok([(
                            "Error".to_owned(),
                            Message::error(format!("Cannot parse '{}' as integer", s)),
                        )]
                        .into());
                    }
                },
                _ => {
                    return Ok([(
                        "Error".to_owned(),
                        Message::error(format!("Cannot convert {:?} to integer", input)),
                    )]
                    .into());
                }
            };

            Ok([("Out".to_owned(), result)].into())
        }
        "float" => {
            let result = match input {
                Message::Boolean(b) => Message::Float(if *b { 1.0 } else { 0.0 }),
                Message::Integer(i) => Message::Float(*i as f64),
                Message::Float(_) => input.clone(),
                Message::String(s) => match s.parse::<f64>() {
                    Ok(f) => Message::Float(f),
                    Err(_) => {
                        return Ok([(
                            "Error".to_owned(),
                            Message::error(format!("Cannot parse '{}' as float", s)),
                        )]
                        .into());
                    }
                },
                _ => {
                    return Ok([(
                        "Error".to_owned(),
                        Message::error(format!("Cannot convert {:?} to float", input)),
                    )]
                    .into());
                }
            };

            Ok([("Out".to_owned(), result)].into())
        }
        "array" => {
            let result = match input {
                Message::Array(_) => input.clone(),
                Message::String(s) => {
                    match serde_json::from_str::<Vec<serde_json::Value>>(s) {
                        Ok(arr) => {
                            use reflow_network::message::EncodableValue;
                            let values: Vec<EncodableValue> = arr.iter().map(|d| EncodableValue::from(d.clone())).collect();
                            Message::array(values)
                        },
                        Err(_) => {
                            // If not a JSON array, create a single-item array
                            use reflow_network::message::EncodableValue;
                            let values = vec![EncodableValue::from(serde_json::Value::String(s.to_string()))];
                            Message::array(values)
                        }
                    }
                }
                _ => {
                    // Convert any other type to a single-item array
                    use reflow_network::message::EncodableValue;
                    let values = vec![EncodableValue::from(serde_json::to_value(input).unwrap_or_default())];
                    Message::array(values)
                }
            };

            Ok([("Out".to_owned(), result)].into())
        }
        "object" => {
            let result = match input {
                Message::Object(_) => input.clone(),
                Message::String(s) => {
                    match serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(s) {
                        Ok(obj) => Message::object(serde_json::to_value(obj)?.into()),
                        Err(_) => {
                            return Ok([(
                                "Error".to_owned(),
                                Message::error(format!("Cannot parse '{}' as object", s)),
                            )]
                            .into());
                        }
                    }
                }
                _ => {
                    return Ok([(
                        "Error".to_owned(),
                        Message::error(format!("Cannot convert {:?} to object", input)),
                    )]
                    .into());
                }
            };

            Ok([("Out".to_owned(), result)].into())
        }
        _ => Ok([(
            "Error".to_owned(),
            Message::error(format!("Unknown target type: {}", target_type)),
        )]
        .into()),
    }
}
