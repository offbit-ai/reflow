//! Rules engine actor for conditional logic evaluation.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::Value;
use std::collections::HashMap;

/// Rules Engine Actor - Processes Zeal rule sets
///
/// Evaluates rules defined in node metadata and triggers actions.
#[actor(
    RulesEngineActor,
    inports::<100>(data),
    outports::<50>(output, matched, unmatched),
    state(MemoryState)
)]
pub async fn rules_engine_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    let config = context.get_config_hashmap();
    let payload = context.get_payload();

    let data = payload
        .get("data")
        .ok_or_else(|| anyhow::anyhow!("No input data provided"))?;

    let rules = config
        .get("rules")
        .or_else(|| config.get("propertyRules"))
        .and_then(|v| v.as_object());

    if let Some(rules_obj) = rules {
        let rule_type = rules_obj
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("IF");

        let empty_vec = Vec::new();
        let groups = rules_obj
            .get("groups")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty_vec);

        // Initial state depends on the combinator. IF needs every
        // group to match (start optimistic, falsify on first miss).
        // OR fires when any group matches (start pessimistic, lift
        // on first hit). The previous `let mut all_match = true;`
        // initialiser was wrong for OR — an OR rule with zero
        // matching groups would silently fire because the loop
        // body never set `all_match = false` in that branch.
        let mut all_match = rule_type != "OR";

        for group in groups {
            let connector = group
                .get("connector")
                .and_then(|v| v.as_str())
                .unwrap_or("AND");

            let empty_vec = Vec::new();
            let rules = group
                .get("rules")
                .and_then(|v| v.as_array())
                .unwrap_or(&empty_vec);

            let group_match = if connector == "AND" {
                rules.iter().all(|rule| evaluate_rule(rule, data))
            } else {
                rules.iter().any(|rule| evaluate_rule(rule, data))
            };

            if rule_type == "OR" {
                if group_match {
                    all_match = true;
                    break;
                }
            } else if !group_match {
                all_match = false;
                break;
            }
        }

        if all_match {
            // Extract the *inner* JSON value for property modification.
            // Previously this serialized via `serde_json::to_value(data)`
            // which uses Message's tagged form (`{"type":"Object",
            // "data": …}`), so `setProperty` writes leaked into the
            // envelope — every consumer had to unwrap. Pull the raw
            // inner instead so the resulting matched packet is a clean
            // Message::Object holding only the data + new properties.
            let mut output_data = match data {
                Message::Object(obj) => serde_json::to_value(obj.as_ref())
                    .unwrap_or(Value::Null),
                Message::Array(arr) => serde_json::to_value(arr.as_ref())
                    .unwrap_or(Value::Null),
                Message::Any(v) => serde_json::to_value(v.as_ref())
                    .unwrap_or(Value::Null),
                Message::Event(v) => serde_json::to_value(v).unwrap_or(Value::Null),
                // Primitives have no fields to modify; pass through as-is.
                other => serde_json::to_value(other).unwrap_or(Value::Null),
            };

            if let Some(set_props) = rules_obj
                .get("actions")
                .and_then(|a| a.get("setProperty"))
                .and_then(|v| v.as_array())
            {
                for prop in set_props {
                    if let (Some(key), Some(value)) =
                        (prop.get("key").and_then(|v| v.as_str()), prop.get("value"))
                    {
                        if let Value::Object(ref mut map) = output_data {
                            map.insert(key.to_string(), value.clone());
                        }
                    }
                }
            }

            if let Some(set_outputs) = rules_obj
                .get("actions")
                .and_then(|a| a.get("setOutput"))
                .and_then(|v| v.as_array())
            {
                for output in set_outputs {
                    if let (Some(port), Some(value)) = (
                        output.get("port").and_then(|v| v.as_str()),
                        output.get("value"),
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

fn evaluate_rule(rule: &Value, data: &Message) -> bool {
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
        _ => false,
    }
}

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
            let items: Vec<EncodableValue> = arr.into_iter().map(|v| v.into()).collect();
            Message::Array(items.into())
        }
        Value::Object(_) => Message::object(EncodableValue::from(value)),
    }
}
