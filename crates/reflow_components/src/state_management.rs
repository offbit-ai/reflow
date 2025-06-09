//! State Management Components
//!
//! This module contains components that handle persistence and state transitions.

use std::{collections::HashMap, sync::Arc};

use actor_macro::actor;
use anyhow::Error;
use parking_lot::Mutex;
use reflow_network::message::EncodableValue;

use crate::{Actor, ActorContext, ActorLoad, ActorBehavior, MemoryState, Message, Port};

/// Stores and retrieves values from persistent state.
///
/// # Inports
/// - `Set`: Value to store
/// - `Key`: Storage key
/// - `Get`: Trigger to retrieve a value
///
/// # Outports
/// - `Value`: Retrieved value
/// - `Changed`: Notification when state changes
///
/// # State
/// - Maintains key-value storage
#[actor(
    StoreActor,
    inports::<100>(Set, Key, Get),
    outports::<50>(Value, Changed),
    state(MemoryState)
)]
async fn store_actor(
   context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();

    let payload = context.get_payload();
    let state = context.get_state();
    let outport_channels = context.get_outports();

    // Handle Set operation
    if payload.contains_key("Set") && payload.contains_key("Key") {
        let value = payload.get("Set").unwrap().clone();
        let key = if let Some(Message::String(k)) = payload.get("Key") {
            k.clone()
        } else {
            // Convert non-string keys to string
            serde_json::to_string(payload.get("Key").unwrap()).unwrap_or_default()
        };

        // Store the value
        {
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                // Get the storage map or create a new one
                let storage = state_data
                    .get("storage")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();

                let mut new_storage = storage;
                new_storage.insert(key.clone(), serde_json::to_value(&value).unwrap());
                state_data.insert("storage", serde_json::Value::Object(new_storage));

                // Signal that state has changed
                result.insert("Changed".to_owned(), Message::String(key));
            }
        }
    }

    // Handle Get operation
    if payload.contains_key("Get") {
        let key = if let Some(Message::String(k)) = payload.get("Key") {
            k.clone()
        } else if let Some(key_msg) = payload.get("Key") {
            // Convert non-string keys to string
            serde_json::to_string(key_msg).unwrap_or_default()
        } else {
            // No key provided
            return Ok(result);
        };

        // Retrieve the value
        let value = {
            let state_lock = state.lock();
            if let Some(state_data) = state_lock.as_any().downcast_ref::<MemoryState>() {
                state_data
                    .get("storage")
                    .and_then(|v| v.as_object())
                    .and_then(|storage| storage.get(&key))
                    .cloned()
            } else {
                None
            }
        };

        if let Some(value_json) = value {
            // Convert JSON value to Message
            let message = value_json.into();

            result.insert("Value".to_owned(), message);
        }
    }

    Ok(result)
}

/// Manages state transitions based on events.
///
/// # Inports
/// - `Event`: Incoming event
/// - `Transition`: State transition to apply
///
/// # Outports
/// - `State`: Current state
/// - `Changed`: Notification when state changes
///
/// # State
/// - Maintains current state and transition rules
#[actor(
    StateMachineActor,
    inports::<100>(Event, Transition),
    outports::<50>(State, Changed),
    state(MemoryState)
)]
async fn state_machine_actor(
   context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();

    let payload = context.get_payload();
    let state = context.get_state();
    let outport_channels = context.get_outports();

    // Handle Transition definition
    if payload.contains_key("Transition") {
        if let Some(Message::Object(transition)) = payload.get("Transition") {
            // Store the transition rule
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                // Get existing transitions or create new ones
                let transitions = state_data
                    .get("transitions")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();

                let mut new_transitions = transitions;

                // Extract from, event, to from the transition object
                if let serde_json::Value::Object(trans_obj) =
                    serde_json::to_value(transition).unwrap()
                {
                    let from = trans_obj
                        .get("from")
                        .and_then(|v| v.as_str())
                        .unwrap_or("*");

                    let event = trans_obj
                        .get("event")
                        .and_then(|v| v.as_str())
                        .unwrap_or("*");

                    let to = trans_obj
                        .get("to")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();

                    // Create transition key
                    let key = format!("{}-{}", from, event);
                    new_transitions.insert(key, serde_json::Value::String(to.to_string()));

                    state_data.insert("transitions", serde_json::Value::Object(new_transitions));
                }
            }
        }
    }

    // Handle Event
    if payload.contains_key("Event") {
        let event = if let Some(Message::String(e)) = payload.get("Event") {
            e.clone()
        } else {
            // Convert non-string events to string
            serde_json::to_string(payload.get("Event").unwrap()).unwrap_or_default()
        };

        // Get current state and apply transition
        let (new_state, changed) = {
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                // Get current state
                let current = state_data
                    .get("current_state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("initial")
                    .to_string();

                // Get transitions
                let transitions = state_data
                    .get("transitions")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();

                // Look for specific transition
                let specific_key = format!("{}-{}", current, event);
                let wildcard_key = format!("*-{}", event);
                let fallback_key = format!("{}-*", current);

                let next_state = transitions
                    .get(&specific_key)
                    .or_else(|| transitions.get(&wildcard_key))
                    .or_else(|| transitions.get(&fallback_key))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                if let Some(next) = next_state {
                    if next != current {
                        // State has changed
                        state_data.insert("current_state", serde_json::Value::String(next.clone()));
                        (next, true)
                    } else {
                        // No change
                        (current, false)
                    }
                } else {
                    // No transition found
                    (current, false)
                }
            } else {
                ("initial".to_string(), false)
            }
        };

        // Output current state
        result.insert("State".to_owned(), Message::String(new_state.clone()));

        // Signal if state changed
        if changed {
            result.insert("Changed".to_owned(), Message::String(new_state));
        }
    } else {
        // No event, just output current state
        let current = {
            let state_lock = state.lock();
            if let Some(state_data) = state_lock.as_any().downcast_ref::<MemoryState>() {
                state_data
                    .get("current_state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("initial")
                    .to_string()
            } else {
                "initial".to_string()
            }
        };

        result.insert("State".to_owned(), Message::String(current));
    }

    Ok(result)
}

/// Accumulates state over time based on inputs.
///
/// # Inports
/// - `Update`: Value to update state with
/// - `Reset`: Signal to reset the accumulator
///
/// # Outports
/// - `State`: Current accumulated state
///
/// # State
/// - Maintains accumulated value
#[actor(
    AccumulatorActor,
    inports::<100>(Update, Reset),
    outports::<50>(State),
    state(MemoryState)
)]
async fn accumulator_actor(
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
            // Reset accumulator
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                // Get accumulator type from state
                let acc_type = state_data
                    .get("accumulator_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("sum");

                // Reset based on type
                match acc_type {
                    "sum" => state_data.insert("value", serde_json::json!(0)),
                    "product" => state_data.insert("value", serde_json::json!(1)),
                    "array" => state_data.insert("value", serde_json::json!([])),
                    "object" => state_data.insert("value", serde_json::json!({})),
                    "string" => state_data.insert("value", serde_json::json!("")),
                    _ => state_data.insert("value", serde_json::json!(null)),
                };
            }
        }
    }

    // Handle update
    if payload.contains_key("Update") {
        let update = payload.get("Update").unwrap();

        // Update accumulator
        let new_value = {
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                // Get accumulator type from state
                let acc_type = state_data
                    .get("accumulator_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("sum");

                // Get current value
                let current = state_data.get("value").cloned();

                // Apply update based on type
                let new_value = match acc_type {
                    "sum" => {
                        let current_num = current.and_then(|v| v.as_f64()).unwrap_or(0.0);

                        let update_num = match update {
                            Message::Integer(i) => *i as f64,
                            Message::Float(f) => *f,
                            Message::String(s) => s.parse::<f64>().unwrap_or(0.0),
                            _ => 0.0,
                        };

                        serde_json::json!(current_num + update_num)
                    }
                    "product" => {
                        let current_num = current.and_then(|v| v.as_f64()).unwrap_or(1.0);

                        let update_num = match update {
                            Message::Integer(i) => *i as f64,
                            Message::Float(f) => *f,
                            Message::String(s) => s.parse::<f64>().unwrap_or(1.0),
                            _ => 1.0,
                        };

                        serde_json::json!(current_num * update_num)
                    }
                    "array" => {
                        let mut current_arr: Vec<EncodableValue> = current
                            .and_then(|v| {
                                Some(
                                    v.as_array()
                                        .unwrap()
                                        .iter()
                                        .map(|e| e.clone().into())
                                        .collect(),
                                )
                            })
                            .unwrap_or_default();

                        current_arr.push(serde_json::to_value(update)?.into());
                        serde_json::json!(current_arr)
                    }
                    "object" => {
                        let mut current_obj = current
                            .as_ref()
                            .and_then(|v| v.as_object())
                            .cloned()
                            .unwrap_or(serde_json::Map::new());

                        if let Message::Object(update_obj) = update {
                            if let serde_json::Value::Object(update_map) =
                                serde_json::to_value(update_obj).unwrap()
                            {
                                for (k, v) in update_map.iter() {
                                    current_obj.insert(k.clone(), v.clone());
                                }
                            }
                        } else {
                            // Add with auto-generated key
                            let key = format!("item_{}", current_obj.len());
                            current_obj.insert(key, serde_json::to_value(update).unwrap());
                        }

                        serde_json::json!(current_obj)
                    }
                    "string" => {
                        let current_str = current.as_ref().and_then(|v| v.as_str()).unwrap_or("");

                        let update_str = match update {
                            Message::String(s) => s.clone(),
                            _ => serde_json::to_string(update).unwrap_or_default(),
                        };

                        serde_json::json!(format!("{}{}", current_str, update_str))
                    }
                    _ => serde_json::to_value(update).unwrap(),
                };

                state_data.insert("value", new_value.clone());
                new_value
            } else {
                serde_json::to_value(update).unwrap()
            }
        };

        // Convert to Message
        let output = new_value.into();

        return Ok([("State".to_owned(), output)].into());
    }

    // If no update or reset, return current state
    let current = {
        let state_lock = state.lock();
        if let Some(state_data) = state_lock.as_any().downcast_ref::<MemoryState>() {
            state_data.get("value").cloned()
        } else {
            None
        }
    };

    if let Some(value) = current {
        // Convert to Message
        let output = value.into();

        Ok([("State".to_owned(), output)].into())
    } else {
        Ok([].into())
    }
}
