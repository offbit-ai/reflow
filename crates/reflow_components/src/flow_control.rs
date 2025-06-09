//! Flow Control Components
//!
//! This module contains components that control the routing and flow of messages.

use std::{collections::HashMap, sync::Arc};

use actor_macro::actor;
use anyhow::Error;
use parking_lot::Mutex;
use reflow_network::{actor::ActorContext, message::EncodableValue};

use crate::{
    Actor, ActorBehavior, ActorLoad, MemoryState, Message,  Port,
};

/// Routes the input message to one of two outputs based on a condition.
///
/// # Inports
/// - `Condition`: Boolean or evaluable value
/// - `Value`: Data to be routed
///
/// # Outports
/// - `True`: Output when condition is truthy
/// - `False`: Output when condition is falsy
#[actor(
    BranchActor,
    inports::<100>(Condition, Value),
    outports::<50>(True, False),
    await_all_inports
)]
async fn branch_actor(
    context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    
    let condition = payload.get("Condition").expect("Expected Condition input");
    let value = payload.get("Value").expect("Expected Value input");
    
    let is_true = match condition {
        Message::Boolean(b) => *b,
        Message::Integer(i) => *i != 0,
        Message::Float(f) => *f != 0.0,
        Message::String(s) => !s.is_empty(),
        Message::Array(arr) => !arr.is_empty(),
        Message::Object(obj) => !serde_json::json!(obj).is_null(),
        Message::Optional(opt) => opt.is_some(),
        _ => false,
    };
    
    let mut result = HashMap::new();
    if is_true {
        result.insert("True".to_owned(), value.clone());
    } else {
        result.insert("False".to_owned(), value.clone());
    }
    
    Ok(result)
}

/// Multi-way branching based on a discrete value.
///
/// # Inports
/// - `Case`: Value determining which branch to take
/// - `Value`: Data to be routed
///
/// # Outports
/// - `Case1` to `CaseN`: Output for specific cases
/// - `Default`: Default output when no cases match
#[actor(
    SwitchActor,
    inports::<100>(Case, Value),
    outports::<50>(Case1, Case2, Case3, Case4, Case5, Default),
    state(MemoryState),
    await_all_inports
)]
async fn switch_actor(
   context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let case = payload.get("Case").expect("Expected Case input");
    let value = payload.get("Value").expect("Expected Value input");
    
    // Get case mappings from state
    let case_mappings = {
        let state = state.lock();
        if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
            state_data.get("case_mappings")
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default()
        } else {
            serde_json::Map::new()
        }
    };
    
    // Convert case to string for matching
    let case_str = match case {
        Message::String(s) => s.clone(),
        _ => serde_json::to_string(case).unwrap_or_default(),
    };
    
    // Find matching case port
    let port_name = if let Some(port) = case_mappings.get(&case_str) {
        port.as_str().unwrap_or("Default").to_string()
    } else {
        // Check if there's a direct case port match
        let case_port = format!("Case{}", case_str);
        if ["Case1", "Case2", "Case3", "Case4", "Case5"].contains(&case_port.as_str()) {
            case_port
        } else {
            "Default".to_string()
        }
    };
    
    let mut result = HashMap::new();
    result.insert(port_name, value.clone());
    
    Ok(result)
}

/// Duplicates incoming message to all output ports.
///
/// # Inports
/// - `In`: Data to be duplicated
///
/// # Outports
/// - `Out`: Array-indexed output ports that receive identical copies
#[actor(
    ForkActor,
    inports::<100>(In),
    outports::<50>(Out)
)]
async fn fork_actor(
    context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    if let Some(input) = payload.get("In") {
        // Simply send the same message to the Out port
        // The Network handles distributing to all connections
        Ok([("Out".to_owned(), input.clone())].into())
    } else {
        Ok([].into())
    }
}

/// Waits for all required inputs before forwarding combined data.
///
/// # Inports
/// - `A`, `B`, `C`, etc.: Input data to be joined
///
/// # Outports
/// - `Out`: Combined data from all inputs
///
/// # Configuration
/// - `mode`: How to combine data (object, array, concatenate)
/// - `await_all`: Whether to require all inputs before outputting
#[actor(
    JoinActor,
    inports::<100>(A, B, C),
    outports::<50>(Out),
    state(MemoryState),
    await_all_inports
)]
async fn join_actor(
   context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    // Get mode from state or use default
    let mode = {
        let state = state.lock();
        if let Some(state) = state.as_any().downcast_ref::<MemoryState>() {
            state.get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("object").to_string()
        } else {
            "object".to_string()
        }
    };
    
    // Process based on the mode
    match mode.as_str() {
        "object" => {
            let mut result = serde_json::Map::new();
            for (port, message) in payload.iter() {
                let msg:serde_json::Value = serde_json::to_value(message).unwrap_or_default();
                result.insert(port.clone(), msg);
            }
            let result = serde_json::Value::Object(result);
            Ok([("Out".to_owned(), Message::Object(result.into()))].into())
        },
        "array" => {
            let values: Vec<EncodableValue> = payload.values()
                .map(|m| serde_json::to_value(m).unwrap_or_default().into())
                .collect();
            Ok([("Out".to_owned(), Message::Array(values))].into())
        },
        "concatenate" => {
            // Attempt to concatenate strings
            let mut result = String::new();
            for message in payload.values() {
                if let Message::String(s) = message {
                    result.push_str(s);
                } else {
                    result.push_str(&serde_json::to_string(message).unwrap_or_default());
                }
            }
            Ok([("Out".to_owned(), Message::String(result))].into())
        },
        _ => Err(Error::msg(format!("Unknown join mode: {}", mode)))
    }
}

/// Forwards any input message to a single output port.
///
/// # Inports
/// - `In1`, `In2`, `In3`, etc.: Multiple input sources
///
/// # Outports
/// - `Out`: Unified output of all inputs
#[actor(
    MergeActor,
    inports::<100>(In1, In2, In3),
    outports::<50>(Out)
)]
async fn merge_actor(
    context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    // Forward the first available message
    for (_, message) in payload.iter() {
        return Ok([("Out".to_owned(), message.clone())].into());
    }
    
    Ok([].into())
}

/// Passes only messages that meet specified criteria.
///
/// # Inports
/// - `In`: Data to be filtered
/// - `Condition`: Optional filter condition
///
/// # Outports
/// - `Passed`: Messages that passed the filter
/// - `Failed`: Optional port for rejected messages
#[actor(
    FilterActor,
    inports::<100>(In, Condition),
    outports::<50>(Passed, Failed),
    state(MemoryState)
)]
async fn filter_actor(
   context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let input = match payload.get("In") {
        Some(msg) => msg,
        None => return Ok([].into()), // No input, no output
    };
    
    // Get condition from payload or state
    let condition = if let Some(cond) = payload.get("Condition") {
        cond
    } else {
        let state = state.lock();
        if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
            if let Some(cond) = state_data.get("condition") {
                // Convert JSON value to Message
                match cond {
                    serde_json::Value::Bool(b) => &Message::Boolean(*b),
                    _ => &Message::Boolean(false),
                }
            } else {
                &Message::Boolean(true) // Default to pass-through if no condition
            }
        } else {
            &Message::Boolean(true)
        }
    };
    
    // Evaluate condition
    let passes = match condition {
        Message::Boolean(b) => *b,
        Message::Integer(i) => *i != 0,
        Message::Float(f) => *f != 0.0,
        Message::String(s) => !s.is_empty(),
        _ => false,
    };
    
    let mut result = HashMap::new();
    if passes {
        result.insert("Passed".to_owned(), input.clone());
    } else {
        result.insert("Failed".to_owned(), input.clone());
    }
    
    Ok(result)
}

/// Repeats processing until condition is met.
///
/// # Inports
/// - `Initial`: Starting value
/// - `Condition`: Loop continuation test
/// - `Function`: Operation to perform each iteration
///
/// # Outports
/// - `Result`: Final output after loop completion
/// - `Iteration`: Current iteration value (loops back internally)
///
/// # State
/// - Maintains current iteration value
#[actor(
    LoopActor,
    inports::<100>(Initial, Condition, Function, Iteration),
    outports::<50>(Result, Iteration),
    state(MemoryState)
)]
async fn loop_actor(
   context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    // Check if we're starting a new loop or continuing an iteration
    let is_initial = payload.contains_key("Initial");
    let has_iteration = payload.contains_key("Iteration");
    
    let current_value = if is_initial {
        // Starting a new loop with initial value
        payload.get("Initial").unwrap().clone()
    } else if has_iteration {
        // Continuing with iteration value
        payload.get("Iteration").unwrap().clone()
    } else {
        // No input, no output
        return Ok([].into());
    };
    
    // Check condition
    let condition = if let Some(cond) = payload.get("Condition") {
        cond
    } else {
        // Get from state or default to false
        let state = state.lock();
        if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
            if let Some(cond) = state_data.get("condition") {
                match cond {
                    serde_json::Value::Bool(b) => if *b { &Message::Boolean(true) } else { &Message::Boolean(false) },
                    _ => &Message::Boolean(false),
                }
            } else {
                &Message::Boolean(false)
            }
        } else {
            &Message::Boolean(false)
        }
    };
    
    // Evaluate condition
    let continue_loop = match condition {
        Message::Boolean(b) => *b,
        Message::Integer(i) => *i != 0,
        Message::Float(f) => *f != 0.0,
        Message::String(s) => !s.is_empty(),
        _ => false,
    };
    
    if continue_loop {
        // Apply function to current value if provided
        let next_value = if let Some(_func) = payload.get("Function") {
            // Here we would apply the function to current_value
            // For now, just pass through the current value
            current_value.clone()
        } else {
            current_value.clone()
        };
        
        // Send to iteration port for next loop
        let mut result = HashMap::new();
        result.insert("Iteration".to_owned(), next_value);
        Ok(result)
    } else {
        // Loop complete, send final result
        let mut result = HashMap::new();
        result.insert("Result".to_owned(), current_value);
        Ok(result)
    }
}