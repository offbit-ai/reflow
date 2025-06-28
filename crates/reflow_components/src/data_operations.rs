//! Data Operation Components
//!
//! This module contains components that transform, analyze, and manipulate data.

use std::{collections::HashMap, sync::Arc};

use actor_macro::actor;
use anyhow::Error;
use parking_lot::Mutex;
use reflow_network::{actor::ActorContext, message::EncodableValue};
use serde_json::to_string;

use crate::{Actor, ActorLoad, ActorBehavior, MemoryState, Message, Port};
use reflow_network::actor::ActorConfig;

/// Applies a transformation function to input data.
///
/// # Inports
/// - `In`: Data to transform
/// - `Function`: Transformation to apply (optional)
///
/// # Outports
/// - `Out`: Transformed data
///
/// # Configuration
/// - `transform_type`: Predefined transformation function
#[actor(
    TransformActor,
    inports::<100>(In, Function),
    outports::<50>(Out),
    state(MemoryState)
)]
async fn transform_actor(
   context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let input = match payload.get("In") {
        Some(msg) => msg,
        None => return Ok([].into()), // No input, no output
    };

    // Get transform function, first from payload, then from state
    let transform_type = if let Some(Message::String(func)) = payload.get("Function") {
        func.as_str().to_string()
    } else {
        let state = state.lock();
        if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
            state_data
                .get("transform_type")
                .and_then(|v| v.as_str())
                .unwrap_or("identity")
                .to_string()
        } else {
            "identity".to_string()
        }
    };

    let result = match transform_type.as_str() {
        "identity" => input.clone(),
        "uppercase" => {
            if let Message::String(s) = input {
                Message::string(s.to_uppercase())
            } else {
                input.clone()
            }
        }
        "lowercase" => {
            if let Message::String(s) = input {
                Message::string(s.to_lowercase())
            } else {
                input.clone()
            }
        }
        "number_to_string" => match input {
            Message::Integer(i) => Message::string(i.to_string()),
            Message::Float(f) => Message::string(f.to_string()),
            _ => input.clone(),
        },
        "parse_int" => {
            if let Message::String(s) = input {
                if let Ok(i) = s.parse::<i64>() {
                    Message::Integer(i)
                } else {
                    Message::error(format!("Could not parse '{}' as integer", s))
                }
            } else {
                input.clone()
            }
        }
        "parse_float" => {
            if let Message::String(s) = input {
                if let Ok(f) = s.parse::<f64>() {
                    Message::Float(f)
                } else {
                    Message::error(format!("Could not parse '{}' as float", s))
                }
            } else {
                input.clone()
            }
        }
        "to_json" => {
            // Convert any message to JSON string
            Message::string(serde_json::to_string(input).unwrap_or_default())
        }
        "from_json" => {
            // Parse JSON string to appropriate message type
            if let Message::String(s) = input {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                    match v {
                        serde_json::Value::Null => Message::any(serde_json::Value::Null.into()),
                        serde_json::Value::Bool(b) => Message::Boolean(b),
                        serde_json::Value::Number(n) => {
                            if let Some(i) = n.as_i64() {
                                Message::Integer(i)
                            } else if let Some(f) = n.as_f64() {
                                Message::Float(f)
                            } else {
                                Message::error(format!("Could not parse number: {}", n))
                            }
                        }
                        serde_json::Value::String(s) => Message::string(s),
                        serde_json::Value::Array(a) => {
                            Message::array(a.iter().map(|v| v.clone().into()).collect())
                        }
                        serde_json::Value::Object(o) => {
                            Message::object(serde_json::to_value(o)?.into())
                        }
                    }
                } else {
                    Message::error(format!("Could not parse JSON: '{}'", s))
                }
            } else {
                input.clone()
            }
        }
        _ => Message::error(format!("Unknown transform type: {}", transform_type)),
    };

    Ok([("Out".to_owned(), result)].into())
}

/// Applies operation to each item in a collection.
///
/// # Inports
/// - `Collection`: Array or map to process
/// - `Function`: Operation to apply to each item
///
/// # Outports
/// - `Result`: Collection of processed items
#[actor(
    MapActor,
    inports::<100>(Collection, Function),
    outports::<50>(Result),
    state(MemoryState)
)]
async fn map_actor(
    context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let collection = match payload.get("Collection") {
        Some(msg) => msg,
        None => return Ok([].into()), // No input, no output
    };

    // Get transform function, first from payload, then from state
    let transform_type = if let Some(Message::String(func)) = payload.get("Function") {
        func.as_str().to_string()
    } else {
        let state = state.lock();
        if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
            state_data
                .get("transform_type")
                .and_then(|v| v.as_str())
                .unwrap_or("identity")
                .to_string()
        } else {
            "identity".to_string()
        }
    };

    match collection {
        Message::Array(items) => {
            let mapped_items: Vec<EncodableValue> = items
                .iter()
                .map(|item| {
                    // Apply transformation to each item
                    let item_msg = serde_json::to_value(item).unwrap().into();

                    // Apply the same transformation as in transform_actor
                    let result = match transform_type.as_str() {
                        "identity" => item_msg,
                        "uppercase" => {
                            if let Message::String(s) = &item_msg {
                                Message::string(s.to_uppercase())
                            } else {
                                item_msg
                            }
                        }
                        "lowercase" => {
                            if let Message::String(s) = &item_msg {
                                Message::string(s.to_lowercase())
                            } else {
                                item_msg
                            }
                        }
                        "number_to_string" => match &item_msg {
                            Message::Integer(i) => Message::string(i.to_string()),
                            Message::Float(f) => Message::string(f.to_string()),
                            _ => item_msg,
                        },
                        _ => item_msg,
                    };

                    serde_json::to_value(&result).unwrap_or_default().into()
                })
                .collect();

            Ok([("Result".to_owned(), Message::array(mapped_items))].into())
        }
        Message::Object(obj) => {
            if let serde_json::Value::Object(map) = serde_json::to_value(obj).unwrap() {
                let mut result_map = serde_json::Map::new();

                for (key, value) in map.iter() {
                    // Convert value to Message
                    let item_msg = value.clone().into();

                    // Apply transformation
                    let result = match transform_type.as_str() {
                        "identity" => item_msg,
                        "uppercase" => {
                            if let Message::String(s) = &item_msg {
                                Message::string(s.to_uppercase())
                            } else {
                                item_msg
                            }
                        }
                        "lowercase" => {
                            if let Message::String(s) = &item_msg {
                                Message::string(s.to_lowercase())
                            } else {
                                item_msg
                            }
                        }
                        "number_to_string" => match &item_msg {
                            Message::Integer(i) => Message::string(i.to_string()),
                            Message::Float(f) => Message::string(f.to_string()),
                            _ => item_msg,
                        },
                        _ => item_msg,
                    };

                    result_map.insert(
                        key.clone(),
                        serde_json::to_value(&result).unwrap_or_default(),
                    );
                }

                Ok([(
                    "Result".to_owned(),
                    Message::object(serde_json::to_value(result_map)?.into()),
                )]
                .into())
            } else {
                Ok([("Result".to_owned(), collection.clone())].into())
            }
        }
        _ => {
            // For non-collections, just apply the transform once
            let result = match transform_type.as_str() {
                "identity" => collection.clone(),
                "uppercase" => {
                    if let Message::String(s) = collection {
                        Message::string(s.to_uppercase())
                    } else {
                        collection.clone()
                    }
                }
                _ => collection.clone(),
            };

            Ok([("Result".to_owned(), result)].into())
        }
    }
}

/// Combines all items in a collection into a single value.
///
/// # Inports
/// - `Collection`: Array or map to process
/// - `Function`: Reducer function
/// - `Initial`: Optional starting accumulator value
///
/// # Outports
/// - `Result`: Final reduced value
#[actor(
    ReduceActor,
    inports::<100>(Collection, Function, Initial),
    outports::<50>(Result),
    state(MemoryState)
)]
async fn reduce_actor(
    context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let collection = match payload.get("Collection") {
        Some(msg) => msg,
        None => return Ok([].into()), // No input, no output
    };

    // Get reducer function from payload or state
    let reducer_type = if let Some(Message::String(func)) = payload.get("Function") {
        func.clone()
    } else {
        let state = state.lock();
        if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
            state_data
                .get("reducer_type")
                .and_then(|v| v.as_str())
                .unwrap_or("sum")
                .to_string().into()
        } else {
            "sum".to_string().into()
        }
    };

    // Get initial value if provided
    let initial = payload.get("Initial").cloned();

    match collection {
        Message::Array(items) => {
            if items.is_empty() {
                return Ok([(
                    "Result".to_owned(),
                    initial.unwrap_or(Message::any(serde_json::Value::Null.into())),
                )]
                .into());
            }

            // Convert items to Messages for processing
            let items_as_messages: Vec<Message> = items
                .iter()
                .map(|item| serde_json::to_value(item).unwrap().into())
                .collect();

            // Apply reducer based on type
            let result = match reducer_type.as_str() {
                "sum" => {
                    // Initialize with initial value or first item
                    let mut acc = (if let Some(init) = &initial {
                        init
                    } else {
                        &items_as_messages[0]
                    })
                    .clone();

                    // Start from second item if no initial value
                    let start_idx = if initial.is_some() { 0 } else { 1 };

                    for i in start_idx..items_as_messages.len() {
                        let item = &items_as_messages[i];
                        acc = match (&acc, item) {
                            (Message::Integer(a), Message::Integer(b)) => Message::Integer(a + b),
                            (Message::Float(a), Message::Float(b)) => Message::Float(a + b),
                            (Message::Integer(a), Message::Float(b)) => {
                                Message::Float(*a as f64 + b)
                            }
                            (Message::Float(a), Message::Integer(b)) => {
                                Message::Float(a + *b as f64)
                            }
                            (Message::String(a), Message::String(b)) => {
                                Message::string(a.as_str().to_string() + b)
                            }
                            (Message::Array(a), Message::Array(b)) => {
                                let mut result = a.as_ref().clone();
                                result.extend(b.iter().cloned());
                                
                                Message::array(result)
                            }
                            (Message::Object(a), Message::Object(b)) => {
                                if let (
                                    serde_json::Value::Object(a_map),
                                    serde_json::Value::Object(b_map),
                                ) = (
                                    serde_json::to_value(a).unwrap(),
                                    serde_json::to_value(b).unwrap(),
                                ) {
                                    let mut result = a_map.clone();
                                    for (k, v) in b_map.iter() {
                                        result.insert(k.clone(), v.clone());
                                    }
                                    Message::object(serde_json::to_value(result)?.into())
                                } else {
                                    acc.clone()
                                }
                            }
                            _ => acc.clone(), // Incompatible types, keep accumulator
                        };
                    }

                    acc
                }
                "product" => {
                    // Initialize with initial value or first item
                    let mut acc = (if let Some(init) = &initial {
                        init
                    } else {
                        &items_as_messages[0]
                    })
                    .clone();

                    // Start from second item if no initial value
                    let start_idx = if initial.is_some() { 0 } else { 1 };

                    for i in start_idx..items_as_messages.len() {
                        let item = &items_as_messages[i];
                        acc = match (&acc, item) {
                            (Message::Integer(a), Message::Integer(b)) => Message::Integer(a * b),
                            (Message::Float(a), Message::Float(b)) => Message::Float(a * b),
                            (Message::Integer(a), Message::Float(b)) => {
                                Message::Float(*a as f64 * b)
                            }
                            (Message::Float(a), Message::Integer(b)) => {
                                Message::Float(a * *b as f64)
                            }
                            _ => acc, // Incompatible types, keep accumulator
                        };
                    }

                    acc.clone()
                }
                "join" => {
                    // Join strings with separator
                    let separator = {
                        let state = state.lock();
                        if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
                            state_data
                                .get("separator")
                                .and_then(|v| v.as_str())
                                .unwrap_or(",")
                                .to_string()
                        } else {
                            ",".to_string()
                        }
                    };

                    let strings: Vec<String> = items_as_messages
                        .iter()
                        .map(|msg| match msg {
                            Message::String(s) => s.as_str().to_string(),
                            _ => serde_json::to_string(msg).unwrap_or_default(),
                        })
                        .collect();

                    Message::string(strings.join(&separator))
                }
                "max" => {
                    if items_as_messages.is_empty() {
                        initial.unwrap_or(Message::any(serde_json::Value::Null.into()))
                    } else {
                        let mut max_value = items_as_messages[0].clone();

                        for item in &items_as_messages[1..] {
                            max_value = match (&max_value, item) {
                                (Message::Integer(a), Message::Integer(b)) => {
                                    if a < b {
                                        item.clone()
                                    } else {
                                        max_value.clone()
                                    }
                                }
                                (Message::Float(a), Message::Float(b)) => {
                                    if a < b {
                                        item.clone()
                                    } else {
                                        max_value.clone()
                                    }
                                }
                                (Message::Integer(a), Message::Float(b)) => {
                                    if (a.clone() as f64) < *b {
                                        item.clone()
                                    } else {
                                        max_value.clone()
                                    }
                                }
                                (Message::Float(a), Message::Integer(b)) => {
                                    if *a < *b as f64 {
                                        item.clone()
                                    } else {
                                        max_value.clone()
                                    }
                                }
                                (Message::String(a), Message::String(b)) => {
                                    if a < b {
                                        item.clone()
                                    } else {
                                        max_value.clone()
                                    }
                                }
                                _ => max_value.clone(), // Incomparable types, keep current max
                            };
                        }

                        max_value
                    }
                }
                "min" => {
                    if items_as_messages.is_empty() {
                        initial.unwrap_or(Message::Optional(None))
                    } else {
                        let mut min_value = items_as_messages[0].clone();

                        for item in &items_as_messages[1..] {
                            min_value = match (&min_value, item) {
                                (Message::Integer(a), Message::Integer(b)) => {
                                    if a > b {
                                        item.clone()
                                    } else {
                                        min_value.clone()
                                    }
                                }
                                (Message::Float(a), Message::Float(b)) => {
                                    if a > b {
                                        item.clone()
                                    } else {
                                        min_value.clone()
                                    }
                                }
                                (Message::Integer(a), Message::Float(b)) => {
                                    if *a as f64 > *b {
                                        item.clone()
                                    } else {
                                        min_value.clone()
                                    }
                                }
                                (Message::Float(a), Message::Integer(b)) => {
                                    if *a > *b as f64 {
                                        item.clone()
                                    } else {
                                        min_value.clone()
                                    }
                                }
                                (Message::String(a), Message::String(b)) => {
                                    if a > b {
                                        item.clone()
                                    } else {
                                        min_value.clone()
                                    }
                                }
                                _ => min_value.clone(), // Incomparable types, keep current min
                            };
                        }

                        min_value
                    }
                }
                _ => Message::error(format!("Unknown reducer type: {}", reducer_type)),
            };

            Ok([("Result".to_owned(), result)].into())
        }
        _ => {
            // For non-arrays, just return the input
            Ok([("Result".to_owned(), collection.clone())].into())
        }
    }
}

/// Organizes data into categories based on key or criteria.
///
/// # Inports
/// - `Collection`: Array of items to group
/// - `Key`: Property or function to group by
///
/// # Outports
/// - `Result`: Grouped data structure
#[actor(
    GroupActor,
    inports::<100>(Collection, Key),
    outports::<50>(Result),
    state(MemoryState)
)]
async fn group_actor(
   context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();

    let collection = match payload.get("Collection") {
        Some(msg) => msg,
        None => return Ok([].into()), // No input, no output
    };

    // Get key from payload or state
    let key = if let Some(Message::String(k)) = payload.get("Key") {
        k.as_str().to_string()
    } else {
        let state = state.lock();
        if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
            state_data
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        } else {
            "".to_string()
        }
    };

    match collection {
        Message::Array(items) => {
            let mut groups: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

            for item in items.as_ref() {
                let item_value = serde_json::to_value(item).unwrap_or(serde_json::Value::Null);

                // Extract group key
                let group_key = if key.is_empty() {
                    // Use the item itself as the key
                    serde_json::to_string(&item_value).unwrap_or_default()
                } else if let serde_json::Value::Object(obj) = &item_value {
                    // Use the specified property as key
                    if let Some(prop_value) = obj.get(&key) {
                        serde_json::to_string(prop_value).unwrap_or_default()
                    } else {
                        "undefined".to_string()
                    }
                } else {
                    // Can't extract key from non-object
                    "undefined".to_string()
                };

                // Add to group
                if let Some(group) = groups.get_mut(&group_key) {
                    if let serde_json::Value::Array(arr) = group {
                        arr.push(item_value);
                    }
                } else {
                    groups.insert(group_key, serde_json::Value::Array(vec![item_value]));
                }
            }

            Ok([(
                "Result".to_owned(),
                Message::object(serde_json::to_value(groups)?.into()),
            )]
            .into())
        }
        _ => {
            // For non-arrays, just return the input
            Ok([("Result".to_owned(), collection.clone())].into())
        }
    }
}

/// Orders collection data based on criteria.
///
/// # Inports
/// - `Collection`: Array to sort
/// - `Key`: Optional property to sort by
/// - `Order`: Optional sort order ("asc"/"desc")
///
/// # Outports
/// - `Result`: Sorted collection
#[actor(
    SortActor,
    inports::<100>(Collection, Key, Order),
    outports::<50>(Result),
    state(MemoryState)
)]
async fn sort_actor(
    context:ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let collection = match payload.get("Collection") {
        Some(msg) => msg,
        None => return Ok([].into()), // No input, no output
    };

    // Get key from payload or state
    let key = if let Some(Message::String(k)) = payload.get("Key") {
        k.as_ref().to_string()
    } else {
        let state = state.lock();
        if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
            state_data
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        } else {
            "".to_string()
        }
    };

    // Get order from payload or state
    let order = if let Some(Message::String(o)) = payload.get("Order") {
        o.as_str().to_string()
    } else {
        let state = state.lock();
        if let Some(state_data) = state.as_any().downcast_ref::<MemoryState>() {
            state_data
                .get("order")
                .and_then(|v| v.as_str())
                .unwrap_or("asc")
                .to_string()
        } else {
            "asc".to_string()
        }
    };

    let descending = order.to_lowercase() == "desc";

    match collection {
        Message::Array(items) => {
            let mut items_vec = items.as_ref().clone();

            if key.is_empty() {
                // Sort by value directly
                items_vec.sort_by(|a, b| {
                    let cmp = compare_values(a.clone().into(), b.clone().into());
                    if descending {
                        cmp.reverse()
                    } else {
                        cmp
                    }
                });
            } else {
                // Sort by specified property
                items_vec.sort_by(|a, b| {
                    let a_val = extract_property(a.clone().into(), &key);
                    let b_val = extract_property(b.clone().into(), &key);
                    let cmp = compare_values(a_val.clone().into(), b_val.clone().into());
                    if descending {
                        cmp.reverse()
                    } else {
                        cmp
                    }
                });
            }

           
            Ok([("Result".to_owned(), Message::array(items_vec))].into())
        }
        _ => {
            // For non-arrays, just return the input
            Ok([("Result".to_owned(), collection.clone())].into())
        }
    }
}

// Helper function to extract a property from a value
fn extract_property(value: serde_json::Value, key: &str) -> serde_json::Value {
    if let serde_json::Value::Object(obj) = value {
        obj.get(key).cloned().unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    }
}

// Helper function to compare two JSON values
fn compare_values(a: serde_json::Value, b: serde_json::Value) -> std::cmp::Ordering {
    match (a, b) {
        (serde_json::Value::Null, serde_json::Value::Null) => std::cmp::Ordering::Equal,
        (serde_json::Value::Null, _) => std::cmp::Ordering::Less,
        (_, serde_json::Value::Null) => std::cmp::Ordering::Greater,
        (serde_json::Value::Bool(a_val), serde_json::Value::Bool(b_val)) => a_val.cmp(&b_val),
        (serde_json::Value::Bool(_), _) => std::cmp::Ordering::Less,
        (_, serde_json::Value::Bool(_)) => std::cmp::Ordering::Greater,
        (serde_json::Value::Number(a_val), serde_json::Value::Number(b_val)) => {
            if let (Some(a_f), Some(b_f)) = (a_val.as_f64(), b_val.as_f64()) {
                a_f.partial_cmp(&b_f).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                std::cmp::Ordering::Equal
            }
        }
        (serde_json::Value::Number(_), _) => std::cmp::Ordering::Less,
        (_, serde_json::Value::Number(_)) => std::cmp::Ordering::Greater,
        (serde_json::Value::String(a_val), serde_json::Value::String(b_val)) => a_val.cmp(&b_val),
        (serde_json::Value::String(_), _) => std::cmp::Ordering::Less,
        (_, serde_json::Value::String(_)) => std::cmp::Ordering::Greater,
        (serde_json::Value::Array(a_val), serde_json::Value::Array(b_val)) => {
            a_val.len().cmp(&b_val.len())
        }
        (serde_json::Value::Array(_), _) => std::cmp::Ordering::Less,
        (_, serde_json::Value::Array(_)) => std::cmp::Ordering::Greater,
        (serde_json::Value::Object(a_val), serde_json::Value::Object(b_val)) => {
            a_val.len().cmp(&b_val.len())
        }
    }
}
