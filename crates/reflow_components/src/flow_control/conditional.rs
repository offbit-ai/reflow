//! Conditional branching actor for if/else routing.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::ActorContext;
use serde_json::Value;
use std::collections::HashMap;

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

    let data = payload
        .get("data")
        .ok_or_else(|| anyhow::anyhow!("No input data provided"))?;

    let property_values = config.get("propertyValues").and_then(|v| v.as_object());

    // Check for decisionRules first (for tpl_if_branch)
    if let Some(decision_rules) = property_values
        .and_then(|pv| pv.get("decisionRules"))
        .and_then(|v| v.as_object())
    {
        let rule_type = decision_rules
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("IF");

        let groups = decision_rules.get("groups").and_then(|v| v.as_array());

        if let Some(rule_groups) = groups {
            let mut condition_met = false;

            for group in rule_groups {
                let connector = group
                    .get("connector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("AND");

                let rules = group.get("rules").and_then(|v| v.as_array());

                if let Some(rules_array) = rules {
                    let group_match = if connector == "AND" {
                        rules_array
                            .iter()
                            .all(|rule| evaluate_condition(rule, data))
                    } else {
                        rules_array
                            .iter()
                            .any(|rule| evaluate_condition(rule, data))
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

            if condition_met {
                result.insert("true-out".to_string(), data.clone());
            } else {
                result.insert("false-out".to_string(), data.clone());
            }

            return Ok(result);
        }
    }

    // Fallback to simple condition configuration
    let condition_type = property_values
        .and_then(|pv| pv.get("condition_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("equals");

    let condition_value =
        property_values.and_then(|pv| pv.get("condition_value").or_else(|| pv.get("value")));

    let condition_field = property_values
        .and_then(|pv| pv.get("condition_field").or_else(|| pv.get("field")))
        .and_then(|v| v.as_str());

    let condition_met = match condition_type {
        "equals" => {
            if let Some(field) = condition_field {
                if let Message::Object(obj) = data {
                    let obj_value = serde_json::to_value(obj)?;
                    if let Some(field_value) = obj_value.get(field) {
                        condition_value == Some(field_value)
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                let data_value = serde_json::to_value(data)?;
                condition_value == Some(&data_value)
            }
        }
        "greater_than" => match data {
            Message::Integer(i) => condition_value
                .and_then(|v| v.as_i64())
                .is_some_and(|cv| *i > cv),
            Message::Float(f) => condition_value
                .and_then(|v| v.as_f64())
                .is_some_and(|cv| *f > cv),
            _ => false,
        },
        "less_than" => match data {
            Message::Integer(i) => condition_value
                .and_then(|v| v.as_i64())
                .is_some_and(|cv| *i < cv),
            Message::Float(f) => condition_value
                .and_then(|v| v.as_f64())
                .is_some_and(|cv| *f < cv),
            _ => false,
        },
        "contains" => match data {
            Message::String(s) => condition_value
                .and_then(|v| v.as_str())
                .is_some_and(|cv| s.contains(cv)),
            Message::Array(arr) => condition_value.is_some_and(|cv| {
                arr.iter()
                    .any(|item| serde_json::to_value(item).ok().is_some_and(|iv| iv == *cv))
            }),
            _ => false,
        },
        "is_empty" => match data {
            Message::String(s) => s.is_empty(),
            Message::Array(arr) => arr.is_empty(),
            Message::Optional(opt) => opt.is_none(),
            _ => false,
        },
        _ => {
            // Default to simple truthy check
            match data {
                Message::Boolean(b) => *b,
                Message::Integer(i) => *i != 0,
                Message::Float(f) => *f != 0.0,
                Message::String(s) => !s.is_empty(),
                Message::Array(arr) => !arr.is_empty(),
                Message::Optional(opt) => opt.is_some(),
                _ => false,
            }
        }
    };

    if condition_met {
        result.insert("true-out".to_string(), data.clone());
    } else {
        result.insert("false-out".to_string(), data.clone());
    }

    Ok(result)
}

fn evaluate_condition(rule: &Value, data: &Message) -> bool {
    let field = rule.get("field").and_then(|v| v.as_str());
    let operator = rule
        .get("operator")
        .and_then(|v| v.as_str())
        .unwrap_or("is");
    let rule_value = rule.get("value");

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
    } else if let Ok(data_value) = serde_json::to_value(data) {
        Some(data_value)
    } else {
        return false;
    };

    let field_value = match field_value {
        Some(v) => v,
        None => return false,
    };

    match operator {
        "is" => rule_value == Some(&field_value),
        "is_not" => rule_value != Some(&field_value),
        "contains" => match (&field_value, rule_value) {
            (Value::String(s), Some(Value::String(needle))) => s.contains(needle.as_str()),
            (Value::Array(arr), Some(val)) => arr.contains(val),
            _ => false,
        },
        "not_contains" => match (&field_value, rule_value) {
            (Value::String(s), Some(Value::String(needle))) => !s.contains(needle.as_str()),
            (Value::Array(arr), Some(val)) => !arr.contains(val),
            _ => true,
        },
        "greater_than" => match (&field_value, rule_value) {
            (Value::Number(a), Some(Value::Number(b))) => {
                a.as_f64().unwrap_or(0.0) > b.as_f64().unwrap_or(0.0)
            }
            _ => false,
        },
        "less_than" => match (&field_value, rule_value) {
            (Value::Number(a), Some(Value::Number(b))) => {
                a.as_f64().unwrap_or(0.0) < b.as_f64().unwrap_or(0.0)
            }
            _ => false,
        },
        "greater_equal" => match (&field_value, rule_value) {
            (Value::Number(a), Some(Value::Number(b))) => {
                a.as_f64().unwrap_or(0.0) >= b.as_f64().unwrap_or(0.0)
            }
            _ => false,
        },
        "less_equal" => match (&field_value, rule_value) {
            (Value::Number(a), Some(Value::Number(b))) => {
                a.as_f64().unwrap_or(0.0) <= b.as_f64().unwrap_or(0.0)
            }
            _ => false,
        },
        "empty" => match field_value {
            Value::Null => true,
            Value::String(s) => s.is_empty(),
            Value::Array(arr) => arr.is_empty(),
            Value::Object(obj) => obj.is_empty(),
            _ => false,
        },
        "not_empty" => match field_value {
            Value::Null => false,
            Value::String(s) => !s.is_empty(),
            Value::Array(arr) => !arr.is_empty(),
            Value::Object(obj) => !obj.is_empty(),
            _ => true,
        },
        _ => false,
    }
}
