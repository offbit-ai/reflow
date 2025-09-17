//! Zeal Rules Engine Actor
//!
//! Processes rule-based logic from Zeal node templates.

use std::collections::HashMap;
use actor_macro::actor;
use anyhow::{Result, Error};
use reflow_actor::{ActorContext, message::EncodableValue};
use serde_json::Value;
use crate::{Actor, ActorBehavior, ActorLoad, MemoryState, Message, Port};
use std::sync::Arc;
use reflow_actor::ActorConfig;
use reflow_tracing_protocol::client::TracingIntegration;

/// Rules Engine Actor - Processes Zeal rule sets
///
/// Evaluates rules defined in node metadata and triggers actions.
#[actor(
    RulesEngineActor,
    inports::<100>(data),
    outports::<50>(output, matched, unmatched),
    state(MemoryState)
)]
pub async fn rules_engine_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    let config = context.get_config_hashmap();
    let payload = context.get_payload();
    
    // Get input data
    let data = payload.get("data")
        .ok_or_else(|| anyhow::anyhow!("No input data provided"))?;
    
    // Get rules from propertyValues (user-provided values)
    let property_values = config.get("propertyValues")
        .and_then(|v| v.as_object());
    
    let rules = property_values
        .and_then(|pv| pv.get("rules").or_else(|| pv.get("propertyRules")))
        .and_then(|v| v.as_object());
    
    if let Some(rules_obj) = rules {
        // Extract rule groups
        let rule_type = rules_obj.get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("IF");
        
        let empty_vec = Vec::new();
        let groups = rules_obj.get("groups")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty_vec);
        
        let mut all_match = true;
        
        // Process each rule group
        for group in groups {
            let connector = group.get("connector")
                .and_then(|v| v.as_str())
                .unwrap_or("AND");
            
            let empty_vec = Vec::new();
            let rules = group.get("rules")
                .and_then(|v| v.as_array())
                .unwrap_or(&empty_vec);
            
            let group_match = if connector == "AND" {
                // All rules in group must match
                rules.iter().all(|rule| evaluate_rule(rule, data))
            } else {
                // Any rule in group must match
                rules.iter().any(|rule| evaluate_rule(rule, data))
            };
            
            if rule_type == "OR" {
                // OR logic between groups
                if group_match {
                    all_match = true;
                    break;
                }
            } else {
                // AND logic between groups (IF type)
                if !group_match {
                    all_match = false;
                    break;
                }
            }
        }
        
        // Apply actions if rules match
        if all_match {
            let mut output_data = serde_json::to_value(data)?;
            
            // Apply setProperty actions
            if let Some(set_props) = rules_obj.get("actions")
                .and_then(|a| a.get("setProperty"))
                .and_then(|v| v.as_array()) {
                
                for prop in set_props {
                    if let (Some(key), Some(value)) = (
                        prop.get("key").and_then(|v| v.as_str()),
                        prop.get("value")
                    ) {
                        if let Value::Object(ref mut map) = output_data {
                            map.insert(key.to_string(), value.clone());
                        }
                    }
                }
            }
            
            // Apply setOutput actions
            if let Some(set_outputs) = rules_obj.get("actions")
                .and_then(|a| a.get("setOutput"))
                .and_then(|v| v.as_array()) {
                
                for output in set_outputs {
                    if let (Some(port), Some(value)) = (
                        output.get("port").and_then(|v| v.as_str()),
                        output.get("value")
                    ) {
                        result.insert(port.to_string(), json_value_to_message(value.clone()));
                    }
                }
            }
            
            result.insert("matched".to_string(), json_value_to_message(output_data));
        } else {
            result.insert("unmatched".to_string(), data.clone());
        }
    } else {
        // No rules defined, pass through
        result.insert("output".to_string(), data.clone());
    }
    
    Ok(result)
}

// Helper function to evaluate a single rule
fn evaluate_rule(rule: &Value, data: &Message) -> bool {
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
        "between" => {
            if let (Value::Number(n), Some(Value::Array(range))) = (&field_value, rule_value) {
                if range.len() == 2 {
                    let min = range[0].as_f64().unwrap_or(f64::MIN);
                    let max = range[1].as_f64().unwrap_or(f64::MAX);
                    let val = n.as_f64().unwrap_or(0.0);
                    val >= min && val <= max
                } else {
                    false
                }
            } else {
                false
            }
        }
        _ => false
    }
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