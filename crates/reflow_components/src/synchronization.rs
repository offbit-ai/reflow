//! Synchronization Components
//!
//! This module contains components that coordinate timing and sequencing of operations.

use std::{collections::HashMap, sync::Arc, time::Duration};

use actor_macro::actor;
use anyhow::Error;
use parking_lot::Mutex;

use crate::{Actor, ActorBehavior, ActorPayload, ActorState, MemoryState, Message, Network, Port};

/// Delays execution or creates timed events.
///
/// # Inports
/// - `Start`: Boolean trigger
/// - `Duration`: Time in milliseconds
/// - `Payload`: Data to send when timer elapses
///
/// # Outports
/// - `Elapsed`: Output when timer completes
///
/// # State
/// - Tracks timer status
#[actor(
    TimerActor,
    inports::<100>(Start, Duration, Payload),
    outports::<50>(Elapsed),
    state(MemoryState)
)]
async fn timer_actor(
    payload: ActorPayload,
    state: Arc<Mutex<dyn ActorState>>,
    outport_channels: Port,
) -> Result<HashMap<String, Message>, Error> {
    // Check for start signal
    let start = payload
        .get("Start")
        .and_then(|m| match m {
            Message::Boolean(b) => Some(*b),
            Message::Integer(i) => Some(*i != 0),
            Message::String(s) => Some(!s.is_empty()),
            _ => None,
        })
        .unwrap_or(false);

    if !start {
        return Ok([].into());
    }

    // Get duration in milliseconds
    let duration_ms = payload
        .get("Duration")
        .and_then(|m| match m {
            Message::Integer(i) => Some(*i),
            Message::Float(f) => Some(*f as i64),
            Message::String(s) => s.parse::<i64>().ok(),
            _ => None,
        })
        .unwrap_or(1000); // Default: 1 second

    // Get payload to send when timer elapses
    let timer_payload = payload
        .get("Payload")
        .cloned()
        .unwrap_or(Message::Any(serde_json::Value::Null.into()));

    let outports = outport_channels.clone();

    // Store timer info in state
    {
        let mut state_lock = state.lock();
        if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
            state_data.insert("duration", serde_json::json!(duration_ms));
            state_data.insert("running", serde_json::json!(true));
            state_data.insert("payload", serde_json::to_value(&timer_payload).unwrap());
        }
    }

    // Spawn a timer task
    #[cfg(not(target_arch = "wasm32"))]
    {
        let state_clone = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(duration_ms as u64)).await;

            // Set running to false
            {
                let mut state_lock = state_clone.lock();
                if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                    state_data.insert("running", serde_json::json!(false));
                }
            }

            // Send message to output port
            outports
                .0
                .send([("Elapsed".to_owned(), timer_payload)].into())
        });
    }

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen_futures::spawn_local;

        let state_clone = state.clone();
        let timeout_callback = Closure::once(move || {
            // Set running to false
            {
                let mut state_lock = state_clone.lock();
                if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                    state_data.insert("running", serde_json::json!(false));
                }
            }

            // Send message to output port
            let _ =
                Network::send_outport_msg(outports, [("Elapsed".to_owned(), timer_payload)].into());
        });

        let window = web_sys::window().expect("no global window exists");
        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                timeout_callback.as_ref().unchecked_ref(),
                duration_ms as i32,
            )
            .expect("Failed to set timeout");

        timeout_callback.forget(); // Prevent closure from being dropped
    }

    // Return empty result since output is sent asynchronously
    Ok([].into())
}

/// Limits the frequency of rapidly occurring events.
///
/// # Inports
/// - `In`: Input stream
/// - `Wait`: Debounce period in milliseconds
///
/// # Outports
/// - `Out`: Debounced output
///
/// # State
/// - Tracks last event time and pending messages
#[actor(
    DebounceActor,
    inports::<100>(In, Wait),
    outports::<50>(Out),
    state(MemoryState)
)]
async fn debounce_actor(
    payload: ActorPayload,
    state: Arc<Mutex<dyn ActorState>>,
    outport_channels: Port,
) -> Result<HashMap<String, Message>, Error> {
    if !payload.contains_key("In") {
        return Ok([].into()); // No input, no output
    }

    let input = payload.get("In").unwrap().clone();

    // Get wait time in milliseconds
    let wait_ms = payload
        .get("Wait")
        .and_then(|m| match m {
            Message::Integer(i) => Some(*i),
            Message::Float(f) => Some(*f as i64),
            Message::String(s) => s.parse::<i64>().ok(),
            _ => None,
        })
        .unwrap_or(500); // Default: 500ms

    // Store the latest input and timestamp
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    {
        let mut state_lock = state.lock();
        if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
            state_data.insert("last_input", serde_json::to_value(&input).unwrap());
            state_data.insert("last_time", serde_json::json!(now));
            state_data.insert("wait_ms", serde_json::json!(wait_ms));

            // Check if a timer is already running
            let timer_running = state_data
                .get("timer_running")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if !timer_running {
                state_data.insert("timer_running", serde_json::json!(true));

                // Start a new timer
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let state_clone = state.clone();
                    let outports = outport_channels.clone();

                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(wait_ms as u64)).await;

                        let output_message = {
                            let mut state_lock = state_clone.lock();
                            if let Some(state_data) =
                                state_lock.as_mut_any().downcast_mut::<MemoryState>()
                            {
                                state_data.insert("timer_running", serde_json::json!(false));

                                // Get the latest input
                                let last_input = state_data.get("last_input").cloned();
                                if let Some(value) = last_input {
                                    value.into()
                                } else {
                                    return; // No message to send
                                }
                            } else {
                                return; // No state
                            }
                        };

                        // Send the message
                        let _ = outports.0.send([("Out".to_owned(), output_message)].into());
                    });
                }

                #[cfg(target_arch = "wasm32")]
                {
                    use wasm_bindgen::prelude::*;

                    let state_clone = state.clone();
                    let outports = outport_channels.clone();

                    let timeout_callback = Closure::once(move || {
                        let output_message = {
                            let mut state_lock = state_clone.lock();
                            if let Some(state_data) =
                                state_lock.as_mut_any().downcast_mut::<MemoryState>()
                            {
                                state_data.insert("timer_running", serde_json::json!(false));

                                // Get the latest input
                                let last_input = state_data.get("last_input").cloned();
                                if let Some(value) = last_input {
                                    match value {
                                        serde_json::Value::Null => {
                                            Message::Any(serde_json::Value::Null)
                                        }
                                        serde_json::Value::Bool(b) => Message::Boolean(b),
                                        serde_json::Value::Number(n) => {
                                            if let Some(i) = n.as_i64() {
                                                Message::Integer(i)
                                            } else if let Some(f) = n.as_f64() {
                                                Message::Float(f)
                                            } else {
                                                Message::Any(n.into())
                                            }
                                        }
                                        serde_json::Value::String(s) => Message::String(s),
                                        serde_json::Value::Array(a) => Message::Array(a),
                                        serde_json::Value::Object(o) => Message::Object(o.into()),
                                    }
                                } else {
                                    return; // No message to send
                                }
                            } else {
                                return; // No state
                            }
                        };

                        // Send the message
                        let _ = Network::send_outport_msg(
                            outports,
                            [("Out".to_owned(), output_message)].into(),
                        );
                    });

                    let window = web_sys::window().expect("no global window exists");
                    window
                        .set_timeout_with_callback_and_timeout_and_arguments_0(
                            timeout_callback.as_ref().unchecked_ref(),
                            wait_ms as i32,
                        )
                        .expect("Failed to set timeout");

                    timeout_callback.forget(); // Prevent closure from being dropped
                }
            }
        }
    }

    // Return empty result since output is sent asynchronously
    Ok([].into())
}

/// Rate-limits messages to a maximum frequency.
///
/// # Inports
/// - `In`: Input stream
/// - `Rate`: Maximum messages per time unit
///
/// # Outports
/// - `Out`: Rate-limited output
///
/// # State
/// - Tracks message timing
#[actor(
    ThrottleActor,
    inports::<100>(In, Rate),
    outports::<50>(Out),
    state(MemoryState)
)]
async fn throttle_actor(
    payload: ActorPayload,
    state: Arc<Mutex<dyn ActorState>>,
    _outport_channels: Port,
) -> Result<HashMap<String, Message>, Error> {
    if !payload.contains_key("In") {
        return Ok([].into()); // No input, no output
    }

    let input = payload.get("In").unwrap().clone();

    // Get rate in milliseconds between messages
    let rate_ms = payload
        .get("Rate")
        .and_then(|m| match m {
            Message::Integer(i) => Some(1000 / i.max(&1)), // Convert messages/sec to ms between messages
            Message::Float(f) => Some((1000.0 / f.max(1.0)) as i64),
            Message::String(s) => s.parse::<i64>().ok().map(|r| 1000 / r.max(1)),
            _ => None,
        })
        .unwrap_or(1000); // Default: 1 message per second

    // Get current time
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    // Check if enough time has passed since last message
    let should_send = {
        let mut state_lock = state.lock();
        if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
            let last_time = state_data
                .get("last_time")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            let elapsed = now - last_time;

            if elapsed >= rate_ms {
                // Update last send time
                state_data.insert("last_time", serde_json::json!(now));
                true
            } else {
                false
            }
        } else {
            true // No state, allow sending
        }
    };

    if should_send {
        Ok([("Out".to_owned(), input)].into())
    } else {
        Ok([].into()) // Skip this message due to rate limiting
    }
}

/// Accumulates messages until triggered to release.
///
/// # Inports
/// - `In`: Data to buffer
/// - `Release`: Signal to output buffered data
///
/// # Outports
/// - `Out`: Collected data when released
///
/// # State
/// - Maintains buffer of messages
///
/// # Configuration
/// - `buffer_type`: Array, newest only, etc.
#[actor(
    BufferActor,
    inports::<100>(In, Release),
    outports::<50>(Out),
    state(MemoryState)
)]
async fn buffer_actor(
    payload: ActorPayload,
    state: Arc<Mutex<dyn ActorState>>,
    _outport_channels: Port,
) -> Result<HashMap<String, Message>, Error> {
    // Check if we have new data to buffer
    if payload.contains_key("In") {
        let input = payload.get("In").unwrap().clone();

        // Get buffer type from state
        let buffer_type = {
            let state_lock = state.lock();
            if let Some(state_data) = state_lock.as_any().downcast_ref::<MemoryState>() {
                state_data
                    .get("buffer_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("array")
                    .to_string()
            } else {
                "array".to_string()
            }
        };

        // Update buffer based on type
        {
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                match buffer_type.as_str() {
                    "array" => {
                        // Append to array buffer
                        let buffer = state_data
                            .get("buffer")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();

                        let mut new_buffer = buffer;
                        new_buffer.push(serde_json::to_value(&input).unwrap());
                        state_data.insert("buffer", serde_json::Value::Array(new_buffer));
                    }
                    "newest" => {
                        // Keep only the newest value
                        state_data.insert("buffer", serde_json::to_value(&input).unwrap());
                    }
                    "object" => {
                        // Merge into object buffer
                        let buffer = state_data
                            .get("buffer")
                            .and_then(|v| v.as_object())
                            .cloned()
                            .unwrap_or_default();

                        let mut new_buffer = buffer;

                        // If input is an object, merge it
                        if let Message::Object(obj) = &input {
                            if let serde_json::Value::Object(obj_map) =
                                serde_json::to_value(obj).unwrap()
                            {
                                for (k, v) in obj_map.iter() {
                                    new_buffer.insert(k.clone(), v.clone());
                                }
                            }
                        } else {
                            // Add with auto-generated key
                            let key = format!("item_{}", new_buffer.len());
                            new_buffer.insert(key, serde_json::to_value(&input).unwrap());
                        }

                        state_data.insert("buffer", serde_json::Value::Object(new_buffer));
                    }
                    _ => {
                        // Default to array
                        let buffer = state_data
                            .get("buffer")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();

                        let mut new_buffer = buffer;
                        new_buffer.push(serde_json::to_value(&input).unwrap());
                        state_data.insert("buffer", serde_json::Value::Array(new_buffer));
                    }
                }
            }
        }
    }

    // Check if we should release the buffer
    if payload.contains_key("Release") {
        let release = payload
            .get("Release")
            .and_then(|m| match m {
                Message::Boolean(b) => Some(*b),
                Message::Integer(i) => Some(*i != 0),
                Message::String(s) => Some(!s.is_empty()),
                _ => Some(true), // Any non-boolean value triggers release
            })
            .unwrap_or(true);

        if release {
            // Get and clear buffer
            let buffer = {
                let mut state_lock = state.lock();
                if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                    let buffer_type = state_data
                        .get("buffer_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("array");

                    let buffer = state_data.get("buffer").cloned();

                    // Reset buffer
                    match buffer_type {
                        "array" => state_data.insert("buffer", serde_json::json!([])),
                        "newest" => state_data.insert("buffer", serde_json::json!(null)),
                        "object" => state_data.insert("buffer", serde_json::json!({})),
                        _ => state_data.insert("buffer", serde_json::json!([])),
                    };

                    buffer
                } else {
                    None
                }
            };

            if let Some(buffer_value) = buffer {
                // Convert to appropriate Message type
                let output = buffer_value.into();

                return Ok([("Out".to_owned(), output)].into());
            }
        }
    }

    Ok([].into())
}
