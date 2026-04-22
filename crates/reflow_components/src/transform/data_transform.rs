//! Data transform actor for simple field-level transformations.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Data Transform Actor - Compatible with tpl_data_transformer
///
/// Transforms data based on Zeal configuration.
#[actor(
    DataTransformActor,
    inports::<100>(input),
    outports::<50>(output, error),
    state(MemoryState)
)]
pub async fn data_transform_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    let config = context.get_config_hashmap();
    let payload = context.get_payload();

    let input = payload
        .get("input")
        .ok_or_else(|| anyhow::anyhow!("No input data provided"))?;

    let transform_type = config
        .get("transform_type")
        .or_else(|| config.get("operation"))
        .and_then(|v| v.as_str())
        .unwrap_or("passthrough");

    let transformed = match transform_type {
        "to_uppercase" => match input {
            Message::String(s) => Message::String(s.to_uppercase().into()),
            _ => input.clone(),
        },
        "to_lowercase" => match input {
            Message::String(s) => Message::String(s.to_lowercase().into()),
            _ => input.clone(),
        },
        "to_json" => {
            let json_value = serde_json::to_value(input)?;
            Message::String(json_value.to_string().into())
        }
        "from_json" => match input {
            Message::String(s) => match serde_json::from_str::<Value>(s) {
                Ok(val) => json_value_to_message(val),
                Err(e) => {
                    result.insert(
                        "error".to_string(),
                        Message::Error(format!("JSON parse error: {}", e).into()),
                    );
                    return Ok(result);
                }
            },
            _ => input.clone(),
        },
        "extract_field" => {
            let field_name = config
                .get("field_name")
                .or_else(|| config.get("field"))
                .and_then(|v| v.as_str());

            if let Some(field) = field_name {
                if let Message::Object(obj) = input {
                    let obj_value = serde_json::to_value(obj)?;
                    if let Some(field_value) = obj_value.get(field) {
                        json_value_to_message(field_value.clone())
                    } else {
                        Message::Optional(None)
                    }
                } else {
                    input.clone()
                }
            } else {
                input.clone()
            }
        }
        "set_field" => {
            let field_name = config
                .get("field_name")
                .or_else(|| config.get("field"))
                .and_then(|v| v.as_str());

            let field_value = config.get("field_value").or_else(|| config.get("value"));

            if let (Some(field), Some(value)) = (field_name, field_value) {
                let mut obj_value = if let Message::Object(obj) = input {
                    serde_json::to_value(obj)?
                } else {
                    json!({})
                };

                if let Value::Object(ref mut map) = obj_value {
                    map.insert(field.to_string(), value.clone());
                }

                Message::object(EncodableValue::from(obj_value))
            } else {
                input.clone()
            }
        }
        "template" => {
            let template = config
                .get("template")
                .and_then(|v| v.as_str())
                .unwrap_or("${value}");

            let input_str = match input {
                Message::String(s) => s.to_string(),
                _ => serde_json::to_string(input)?,
            };

            let output = template.replace("${value}", &input_str);
            Message::String(output.into())
        }
        _ => input.clone(), // passthrough
    };

    result.insert("output".to_string(), transformed);
    Ok(result)
}

pub(crate) fn json_value_to_message(value: Value) -> Message {
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
