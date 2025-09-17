//! Zeal Flow Control Category Actors
//!
//! Actors for controlling data flow following Zeal template specifications.

use std::collections::HashMap;
use actor_macro::actor;
use anyhow::{Result, Error};
use reflow_actor::{ActorContext, message::EncodableValue};
use serde_json::{json, Value};
use crate::{Actor, ActorBehavior, ActorLoad, MemoryState, Message, Port};
use std::sync::Arc;
use reflow_actor::ActorConfig;
use reflow_tracing_protocol::client::TracingIntegration;


/// Conditional Branch Actor - Compatible with tpl_if_branch
///
/// Routes data based on a condition specified in Zeal configuration.
#[actor(
    ConditionalBranchActor,
    inports::<100>(data),
    outports::<50>(true_output, false_output),
    state(MemoryState)
)]
pub async fn conditional_branch_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    let config = context.get_config_hashmap();
    let payload = context.get_payload();
    
    // Get input data
    let data = payload.get("data")
        .ok_or_else(|| anyhow::anyhow!("No input data provided"))?;
    
    // Get propertyValues (user-provided values)
    let property_values = config.get("propertyValues")
        .and_then(|v| v.as_object());
    
    // Check for decisionRules first (for tpl_if_branch)
    if let Some(decision_rules) = property_values
        .and_then(|pv| pv.get("decisionRules"))
        .and_then(|v| v.as_object()) {
        
        // Process decision rules (similar to rules engine)
        // Decision rules have the same structure as regular rules
        let rule_type = decision_rules.get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("IF");
        
        let groups = decision_rules.get("groups")
            .and_then(|v| v.as_array());
        
        if let Some(rule_groups) = groups {
            let mut condition_met = false;
            
            for group in rule_groups {
                let connector = group.get("connector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("AND");
                
                let rules = group.get("rules")
                    .and_then(|v| v.as_array());
                
                if let Some(rules_array) = rules {
                    let group_match = if connector == "AND" {
                        rules_array.iter().all(|rule| evaluate_condition(rule, data))
                    } else {
                        rules_array.iter().any(|rule| evaluate_condition(rule, data))
                    };
                    
                    if rule_type == "OR" && group_match {
                        condition_met = true;
                        break;
                    } else if rule_type == "IF" && !group_match {
                        condition_met = false;
                        break;
                    } else if rule_type == "IF" {
                        condition_met = true;
                    }
                }
            }
            
            // Route based on decision rules evaluation
            // Use the actual Zeal port IDs
            if condition_met {
                result.insert("true-out".to_string(), data.clone());
            } else {
                result.insert("false-out".to_string(), data.clone());
            }
            
            return Ok(result);
        }
    }
    
    // Fallback to simple condition configuration if no decisionRules
    let condition_type = property_values
        .and_then(|pv| pv.get("condition_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("equals");
    
    let condition_value = property_values
        .and_then(|pv| pv.get("condition_value").or_else(|| pv.get("value")));
    
    let condition_field = property_values
        .and_then(|pv| pv.get("condition_field").or_else(|| pv.get("field")))
        .and_then(|v| v.as_str());
    
    // Evaluate the condition
    let condition_met = match condition_type {
        "equals" => {
            if let Some(field) = condition_field {
                // Check specific field in data
                if let Message::Object(obj) = data {
                    let obj_value = serde_json::to_value(obj)?;
                    if let Some(field_value) = obj_value.get(field) {
                        condition_value.map_or(false, |cv| field_value == cv)
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                // Direct comparison
                let data_value = serde_json::to_value(data)?;
                condition_value.map_or(false, |cv| &data_value == cv)
            }
        }
        "greater_than" => {
            // Numeric comparison
            match data {
                Message::Integer(i) => {
                    condition_value
                        .and_then(|v| v.as_i64())
                        .map_or(false, |cv| *i > cv)
                }
                Message::Float(f) => {
                    condition_value
                        .and_then(|v| v.as_f64())
                        .map_or(false, |cv| *f > cv)
                }
                _ => false
            }
        }
        "less_than" => {
            match data {
                Message::Integer(i) => {
                    condition_value
                        .and_then(|v| v.as_i64())
                        .map_or(false, |cv| *i < cv)
                }
                Message::Float(f) => {
                    condition_value
                        .and_then(|v| v.as_f64())
                        .map_or(false, |cv| *f < cv)
                }
                _ => false
            }
        }
        "contains" => {
            // String or array contains
            match data {
                Message::String(s) => {
                    condition_value
                        .and_then(|v| v.as_str())
                        .map_or(false, |cv| s.contains(cv))
                }
                Message::Array(arr) => {
                    condition_value.map_or(false, |cv| {
                        arr.iter().any(|item| {
                            serde_json::to_value(item).ok()
                                .map_or(false, |iv| iv == *cv)
                        })
                    })
                }
                _ => false
            }
        }
        "is_empty" => {
            match data {
                Message::String(s) => s.is_empty(),
                Message::Array(arr) => arr.is_empty(),
                Message::Optional(opt) => opt.is_none(),
                _ => false
            }
        }
        _ => {
            // Default to simple truthy check
            match data {
                Message::Boolean(b) => *b,
                Message::Integer(i) => *i != 0,
                Message::Float(f) => *f != 0.0,
                Message::String(s) => !s.is_empty(),
                Message::Array(arr) => !arr.is_empty(),
                Message::Optional(opt) => opt.is_some(),
                _ => false
            }
        }
    };
    
    // Route to appropriate output
    // Use the actual Zeal port IDs
    if condition_met {
        result.insert("true-out".to_string(), data.clone());
    } else {
        result.insert("false-out".to_string(), data.clone());
    }
    
    Ok(result)
}

// Helper function to evaluate a condition rule
fn evaluate_condition(rule: &Value, data: &Message) -> bool {
    let field = rule.get("field").and_then(|v| v.as_str());
    let operator = rule.get("operator").and_then(|v| v.as_str()).unwrap_or("is");
    let rule_value = rule.get("value");
    
    // Get the field value from data
    let field_value = if let Some(field_name) = field {
        if let Message::Object(obj) = data {
            if let Ok(obj_value) = serde_json::to_value(obj) {
                obj_value.get(field_name).cloned()
            } else {
                return false;
            }
        } else {
            None
        }
    } else {
        if let Ok(data_value) = serde_json::to_value(data) {
            Some(data_value)
        } else {
            return false;
        }
    };
    
    let field_value = match field_value {
        Some(v) => v,
        None => return false
    };
    
    match operator {
        "is" => rule_value.map_or(false, |v| &field_value == v),
        "is_not" => rule_value.map_or(true, |v| &field_value != v),
        "contains" => {
            match (&field_value, rule_value) {
                (Value::String(s), Some(Value::String(needle))) => s.contains(needle.as_str()),
                (Value::Array(arr), Some(val)) => arr.contains(val),
                _ => false
            }
        }
        "not_contains" => {
            match (&field_value, rule_value) {
                (Value::String(s), Some(Value::String(needle))) => !s.contains(needle.as_str()),
                (Value::Array(arr), Some(val)) => !arr.contains(val),
                _ => true
            }
        }
        "greater_than" => {
            match (&field_value, rule_value) {
                (Value::Number(a), Some(Value::Number(b))) => {
                    a.as_f64().unwrap_or(0.0) > b.as_f64().unwrap_or(0.0)
                }
                _ => false
            }
        }
        "less_than" => {
            match (&field_value, rule_value) {
                (Value::Number(a), Some(Value::Number(b))) => {
                    a.as_f64().unwrap_or(0.0) < b.as_f64().unwrap_or(0.0)
                }
                _ => false
            }
        }
        "greater_equal" => {
            match (&field_value, rule_value) {
                (Value::Number(a), Some(Value::Number(b))) => {
                    a.as_f64().unwrap_or(0.0) >= b.as_f64().unwrap_or(0.0)
                }
                _ => false
            }
        }
        "less_equal" => {
            match (&field_value, rule_value) {
                (Value::Number(a), Some(Value::Number(b))) => {
                    a.as_f64().unwrap_or(0.0) <= b.as_f64().unwrap_or(0.0)
                }
                _ => false
            }
        }
        "empty" => {
            match field_value {
                Value::Null => true,
                Value::String(s) => s.is_empty(),
                Value::Array(arr) => arr.is_empty(),
                Value::Object(obj) => obj.is_empty(),
                _ => false
            }
        }
        "not_empty" => {
            match field_value {
                Value::Null => false,
                Value::String(s) => !s.is_empty(),
                Value::Array(arr) => !arr.is_empty(),
                Value::Object(obj) => !obj.is_empty(),
                _ => true
            }
        }
        _ => false
    }
}

/// Switch Case Actor - Compatible with tpl_switch
///
/// Routes data to different outputs based on case matching.
#[actor(
    SwitchCaseActor,
    inports::<100>(data),
    outports::<50>(case1, case2, case3, case4, default),
    state(MemoryState)
)]
pub async fn switch_case_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    let config = context.get_config_hashmap();
    let payload = context.get_payload();
    
    // Get input data
    let data = payload.get("data")
        .ok_or_else(|| anyhow::anyhow!("No input data provided"))?;
    
    // Get propertyValues (user-provided values)
    let property_values = config.get("propertyValues")
        .and_then(|v| v.as_object());
    
    // Get switch field from propertyValues
    let switch_field = property_values
        .and_then(|pv| pv.get("switch_field").or_else(|| pv.get("field")))
        .and_then(|v| v.as_str());
    
    // Extract value to switch on
    let switch_value = if let Some(field) = switch_field {
        // Extract field from object
        if let Message::Object(obj) = data {
            let obj_value = serde_json::to_value(obj)?;
            obj_value.get(field).cloned()
        } else {
            None
        }
    } else {
        // Use the data directly
        Some(serde_json::to_value(data)?)
    };
    
    // Get case mappings from propertyValues
    let case1_value = property_values.and_then(|pv| pv.get("case1_value"));
    let case2_value = property_values.and_then(|pv| pv.get("case2_value"));
    let case3_value = property_values.and_then(|pv| pv.get("case3_value"));
    let case4_value = property_values.and_then(|pv| pv.get("case4_value"));
    
    // Match against cases
    let output_port = match switch_value {
        Some(val) if case1_value.map_or(false, |cv| cv == &val) => "case1",
        Some(val) if case2_value.map_or(false, |cv| cv == &val) => "case2",
        Some(val) if case3_value.map_or(false, |cv| cv == &val) => "case3",
        Some(val) if case4_value.map_or(false, |cv| cv == &val) => "case4",
        _ => "default"
    };
    
    result.insert(output_port.to_string(), data.clone());
    Ok(result)
}

/// Loop Actor - Compatible with tpl_loop
///
/// Iterates over a collection or repeats based on condition.
#[actor(
    LoopActor,
    inports::<100>(collection, initial_value),
    outports::<50>(item, completed),
    state(MemoryState)
)]
pub async fn loop_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    let config = context.get_config_hashmap();
    let payload = context.get_payload();
    let state = context.get_state();
    
    // Check if we have a collection to iterate
    if let Some(Message::Array(collection)) = payload.get("collection") {
        // Get current index from state
        let mut state_lock = state.lock();
        let memory_state = state_lock.as_mut_any()
            .downcast_mut::<MemoryState>()
            .ok_or_else(|| anyhow::anyhow!("Invalid state type"))?;
        
        let current_index = memory_state.get("loop_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        
        if current_index < collection.len() {
            // Output current item
            let item = &collection[current_index];
            result.insert("item".to_string(), 
                Message::object(EncodableValue::from(json!({
                    "value": serde_json::to_value(item)?,
                    "index": current_index
                })))
            );
            
            // Update index for next iteration
            memory_state.insert("loop_index", json!(current_index + 1));
        } else {
            // Loop completed
            result.insert("completed".to_string(), Message::Boolean(true));
            // Reset index
            memory_state.insert("loop_index", json!(0));
        }
    } else {
        // No collection provided
        result.insert("completed".to_string(), Message::Boolean(true));
    }
    
    Ok(result)
}