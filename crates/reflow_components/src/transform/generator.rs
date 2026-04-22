//! Generator actor — produces arrays/sequences from high-level config.
//!
//! Replaces the need for manual for-loops in DAG orchestration.
//! Users configure the generator type and parameters; the actor
//! outputs the generated data as a JSON array.
//!
//! ## Supported types
//!
//! - `"range"` — integer/float sequence: `{ start, end, step }`
//! - `"linspace"` — N evenly spaced values: `{ start, end, count }`
//! - `"repeat"` — N copies of a value: `{ value, count }`
//! - `"index"` — array of objects with `$i` expanded: `{ count, template }`
//! - `"grid"` — 2D/3D grid points: `{ width, height, depth?, spacing }`

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    GeneratorActor,
    inports::<10>(trigger, params),
    outports::<1>(output, metadata),
    state(MemoryState)
)]
pub async fn generator_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let payload = ctx.get_payload();

    // Merge runtime params over config
    let mut params = config.clone();
    if let Some(Message::Object(obj)) = payload.get("params") {
        let v: Value = obj.as_ref().clone().into();
        if let Value::Object(map) = v {
            for (k, v) in map {
                params.insert(k, v);
            }
        }
    }

    let gen_type = params
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("range");

    let items: Vec<Value> = match gen_type {
        "range" => generate_range(&params),
        "linspace" => generate_linspace(&params),
        "repeat" => generate_repeat(&params),
        "index" => generate_index(&params),
        "grid" => generate_grid(&params),
        other => return Err(anyhow::anyhow!("unknown generator type: {}", other)),
    };

    let count = items.len();

    let mut out = HashMap::new();
    out.insert(
        "output".to_string(),
        Message::object(EncodableValue::from(json!(items))),
    );

    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": gen_type,
            "count": count,
        }))),
    );

    Ok(out)
}

fn generate_range(params: &HashMap<String, Value>) -> Vec<Value> {
    let start = params.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let end = params.get("end").and_then(|v| v.as_f64()).unwrap_or(10.0);
    let step = params.get("step").and_then(|v| v.as_f64()).unwrap_or(1.0);

    if step == 0.0 {
        return vec![];
    }

    let mut items = Vec::new();
    let mut v = start;
    if step > 0.0 {
        while v < end {
            items.push(json!(v));
            v += step;
        }
    } else {
        while v > end {
            items.push(json!(v));
            v += step;
        }
    }
    items
}

fn generate_linspace(params: &HashMap<String, Value>) -> Vec<Value> {
    let start = params.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let end = params.get("end").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    if count == 0 {
        return vec![];
    }
    if count == 1 {
        return vec![json!(start)];
    }

    (0..count)
        .map(|i| {
            let t = i as f64 / (count - 1) as f64;
            json!(start + t * (end - start))
        })
        .collect()
}

fn generate_repeat(params: &HashMap<String, Value>) -> Vec<Value> {
    let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let value = params.get("value").cloned().unwrap_or(Value::Null);

    vec![value; count]
}

fn generate_index(params: &HashMap<String, Value>) -> Vec<Value> {
    let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let template = params
        .get("template")
        .cloned()
        .unwrap_or(json!({"index": 0}));

    (0..count)
        .map(|i| expand_template(&template, i, count))
        .collect()
}

fn generate_grid(params: &HashMap<String, Value>) -> Vec<Value> {
    let width = params.get("width").and_then(|v| v.as_u64()).unwrap_or(4) as usize;
    let height = params.get("height").and_then(|v| v.as_u64()).unwrap_or(4) as usize;
    let depth = params.get("depth").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let spacing = params
        .get("spacing")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let center = params
        .get("center")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let offset = if center {
        [
            (width - 1) as f64 * spacing * 0.5,
            (height - 1) as f64 * spacing * 0.5,
            (depth - 1) as f64 * spacing * 0.5,
        ]
    } else {
        [0.0; 3]
    };

    let mut items = Vec::with_capacity(width * height * depth);
    for z in 0..depth {
        for y in 0..height {
            for x in 0..width {
                let px = x as f64 * spacing - offset[0];
                let py = y as f64 * spacing - offset[1];
                let pz = z as f64 * spacing - offset[2];
                if depth > 1 {
                    items.push(json!({ "x": px, "y": py, "z": pz, "ix": x, "iy": y, "iz": z }));
                } else {
                    items.push(json!({ "x": px, "y": py, "ix": x, "iy": y }));
                }
            }
        }
    }
    items
}

/// Expand a JSON template, replacing `"$i"` with the index, `"$t"` with normalized [0,1].
fn expand_template(template: &Value, index: usize, count: usize) -> Value {
    match template {
        Value::String(s) => {
            if s == "$i" {
                json!(index)
            } else if s == "$t" {
                let t = if count > 1 {
                    index as f64 / (count - 1) as f64
                } else {
                    0.0
                };
                json!(t)
            } else {
                let replaced = s.replace("$i", &index.to_string()).replace(
                    "$t",
                    &format!(
                        "{:.6}",
                        if count > 1 {
                            index as f64 / (count - 1) as f64
                        } else {
                            0.0
                        }
                    ),
                );
                json!(replaced)
            }
        }
        Value::Number(n) => {
            // Pass through numbers
            Value::Number(n.clone())
        }
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|v| expand_template(v, index, count))
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let new_key = k.replace("$i", &index.to_string());
                out.insert(new_key, expand_template(v, index, count));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

#[allow(dead_code)]
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
        Value::Array(_) | Value::Object(_) => Message::object(EncodableValue::from(value)),
    }
}
