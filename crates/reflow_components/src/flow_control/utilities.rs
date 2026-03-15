//! Flow control and data utility actors.
//!
//! Common patterns for routing, transforming, and controlling
//! data flow through the actor graph.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

// ── Map (apply operation to each item in array) ─────────────────

/// Applies a configurable operation to each item in an array.
/// Operations: identity, to_string, to_number, extract_field, template.
#[actor(MapActor, inports::<10>(input), outports::<1>(output, error), state(MemoryState))]
pub async fn map_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let operation = config
        .get("operation")
        .and_then(|v| v.as_str())
        .unwrap_or("identity");
    let field = config.get("field").and_then(|v| v.as_str()).unwrap_or("");

    let items = match payload.get("input") {
        Some(Message::Array(arr)) => arr.as_ref().clone(),
        _ => return Ok(error_output("Expected Array on input port")),
    };

    let mapped: Vec<EncodableValue> = items
        .iter()
        .map(|item| {
            let val: serde_json::Value = item.clone().into();
            let result = match operation {
                "to_string" => json!(val.to_string()),
                "to_number" => json!(val.as_f64().unwrap_or(0.0)),
                "extract_field" => val.get(field).cloned().unwrap_or(serde_json::Value::Null),
                _ => val,
            };
            EncodableValue::from(result)
        })
        .collect();

    let mut out = HashMap::new();
    out.insert("output".to_string(), Message::Array(Arc::new(mapped)));
    Ok(out)
}

// ── Filter (keep items matching condition) ──────────────────────

/// Filters an array, keeping items that match a condition.
/// Conditions: not_null, truthy, equals, greater_than, contains.
#[actor(FilterActor, inports::<10>(input), outports::<1>(output, rejected, error), state(MemoryState))]
pub async fn filter_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let condition = config
        .get("condition")
        .and_then(|v| v.as_str())
        .unwrap_or("truthy");
    let field = config.get("field").and_then(|v| v.as_str()).unwrap_or("");
    let compare_value = config.get("value").cloned();

    let items = match payload.get("input") {
        Some(Message::Array(arr)) => arr.as_ref().clone(),
        _ => return Ok(error_output("Expected Array on input port")),
    };

    let mut passed = Vec::new();
    let mut rejected = Vec::new();

    for item in &items {
        let val: serde_json::Value = item.clone().into();
        let test_val = if field.is_empty() {
            &val
        } else {
            val.get(field).unwrap_or(&val)
        };

        let keep = match condition {
            "not_null" => !test_val.is_null(),
            "equals" => compare_value.as_ref().map_or(false, |cv| test_val == cv),
            "greater_than" => {
                let a = test_val.as_f64().unwrap_or(0.0);
                let b = compare_value.as_ref().and_then(|v| v.as_f64()).unwrap_or(0.0);
                a > b
            }
            "less_than" => {
                let a = test_val.as_f64().unwrap_or(0.0);
                let b = compare_value.as_ref().and_then(|v| v.as_f64()).unwrap_or(0.0);
                a < b
            }
            "contains" => {
                let s = test_val.as_str().unwrap_or("");
                let sub = compare_value.as_ref().and_then(|v| v.as_str()).unwrap_or("");
                s.contains(sub)
            }
            _ /* truthy */ => match test_val {
                serde_json::Value::Null => false,
                serde_json::Value::Bool(b) => *b,
                serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
                serde_json::Value::String(s) => !s.is_empty(),
                _ => true,
            },
        };

        if keep {
            passed.push(item.clone());
        } else {
            rejected.push(item.clone());
        }
    }

    let mut out = HashMap::new();
    out.insert("output".to_string(), Message::Array(Arc::new(passed)));
    out.insert("rejected".to_string(), Message::Array(Arc::new(rejected)));
    Ok(out)
}

// ── Reduce (accumulate array to single value) ───────────────────

/// Reduces an array to a single value.
/// Operations: sum, product, min, max, count, concat, first, last.
#[actor(ReduceActor, inports::<10>(input), outports::<1>(output, error), state(MemoryState))]
pub async fn reduce_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let operation = config
        .get("operation")
        .and_then(|v| v.as_str())
        .unwrap_or("sum");

    let items = match payload.get("input") {
        Some(Message::Array(arr)) => arr.as_ref().clone(),
        _ => return Ok(error_output("Expected Array on input port")),
    };

    if items.is_empty() {
        return Ok([("output".to_string(), Message::Float(0.0))].into());
    }

    let values: Vec<serde_json::Value> = items.iter().map(|i| i.clone().into()).collect();

    let result = match operation {
        "sum" => {
            let s: f64 = values.iter().filter_map(|v| v.as_f64()).sum();
            Message::Float(s)
        }
        "product" => {
            let p: f64 = values.iter().filter_map(|v| v.as_f64()).product();
            Message::Float(p)
        }
        "min" => {
            let m = values
                .iter()
                .filter_map(|v| v.as_f64())
                .fold(f64::MAX, f64::min);
            Message::Float(m)
        }
        "max" => {
            let m = values
                .iter()
                .filter_map(|v| v.as_f64())
                .fold(f64::MIN, f64::max);
            Message::Float(m)
        }
        "count" => Message::Integer(values.len() as i64),
        "concat" => {
            let s: String = values
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("");
            Message::String(s.into())
        }
        "first" => {
            let v = values.first().cloned().unwrap_or(serde_json::Value::Null);
            Message::object(EncodableValue::from(v))
        }
        "last" => {
            let v = values.last().cloned().unwrap_or(serde_json::Value::Null);
            Message::object(EncodableValue::from(v))
        }
        _ => Message::Float(0.0),
    };

    Ok([("output".to_string(), result)].into())
}

// ── Merge (combine multiple inputs into one object) ─────────────

/// Merges multiple inputs into a single object or array.
#[actor(MergeActor, inports::<10>(a, b, c, d), outports::<1>(output), state(MemoryState), await_all_inports)]
pub async fn merge_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let mode = config
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("object");

    match mode {
        "array" => {
            let mut arr = Vec::new();
            for port in ["a", "b", "c", "d"] {
                if let Some(msg) = payload.get(port) {
                    let val = serde_json::to_value(msg).unwrap_or(serde_json::Value::Null);
                    arr.push(EncodableValue::from(val));
                }
            }
            Ok([("output".to_string(), Message::Array(Arc::new(arr)))].into())
        }
        _ => {
            let mut obj = serde_json::Map::new();
            for port in ["a", "b", "c", "d"] {
                if let Some(msg) = payload.get(port) {
                    let val = serde_json::to_value(msg).unwrap_or(serde_json::Value::Null);
                    obj.insert(port.to_string(), val);
                }
            }
            Ok([(
                "output".to_string(),
                Message::object(EncodableValue::from(serde_json::Value::Object(obj))),
            )]
            .into())
        }
    }
}

// ── Split (split array into head + tail) ────────────────────────

/// Splits an array into first element and rest.
#[actor(SplitActor, inports::<10>(input), outports::<1>(head, tail, count), state(MemoryState))]
pub async fn split_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();

    let items = match payload.get("input") {
        Some(Message::Array(arr)) => arr.as_ref().clone(),
        _ => return Ok(error_output("Expected Array on input port")),
    };

    let count = items.len();
    let mut out = HashMap::new();

    if let Some(first) = items.first() {
        let val: serde_json::Value = first.clone().into();
        out.insert(
            "head".to_string(),
            Message::object(EncodableValue::from(val)),
        );
    }

    let tail: Vec<EncodableValue> = items.into_iter().skip(1).collect();
    out.insert("tail".to_string(), Message::Array(Arc::new(tail)));
    out.insert("count".to_string(), Message::Integer(count as i64));

    Ok(out)
}

// ── Delay (pass through after configurable delay) ───────────────

/// Delays data forwarding by a configurable duration.
#[actor(DelayActor, inports::<10>(input), outports::<1>(output), state(MemoryState))]
pub async fn delay_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let delay_ms = config
        .get("delayMs")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000);

    let input = payload.get("input").cloned().unwrap_or(Message::Flow);

    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;

    Ok([("output".to_string(), input)].into())
}

// ── Gate (pass or block based on condition) ──────────────────────

/// Conditionally passes or blocks data based on a boolean control signal.
#[actor(GateActor, inports::<10>(input, control), outports::<1>(output, blocked), state(MemoryState), await_all_inports)]
pub async fn gate_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let invert = config
        .get("invert")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let open = match payload.get("control") {
        Some(Message::Boolean(b)) => *b,
        Some(Message::Integer(i)) => *i != 0,
        Some(Message::Float(f)) => *f != 0.0,
        _ => true,
    };

    let pass = if invert { !open } else { open };
    let input = payload.get("input").cloned().unwrap_or(Message::Flow);

    let mut out = HashMap::new();
    if pass {
        out.insert("output".to_string(), input);
    } else {
        out.insert("blocked".to_string(), input);
    }
    Ok(out)
}

// ── Collect (accumulate N items then emit as array) ─────────────

/// Collects N incoming messages into an array, then emits.
#[actor(CollectActor, inports::<100>(input), outports::<1>(output, count), state(MemoryState))]
pub async fn collect_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let state = ctx.get_state();
    let target = config.get("count").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    let mut state_lock = state.lock();
    let memory = state_lock
        .as_mut_any()
        .downcast_mut::<MemoryState>()
        .ok_or_else(|| anyhow::anyhow!("Invalid state"))?;

    // Initialize buffer if needed
    if memory.get("buffer").is_none() {
        memory.insert("buffer", serde_json::Value::Array(Vec::new()));
    }

    if let Some(msg) = payload.get("input") {
        let val = serde_json::to_value(msg).unwrap_or(serde_json::Value::Null);
        if let Some(serde_json::Value::Array(arr)) = memory.get_mut("buffer") {
            arr.push(val);

            if arr.len() >= target {
                let items: Vec<EncodableValue> = arr.drain(..).map(EncodableValue::from).collect();
                let count = items.len();
                let mut out = HashMap::new();
                out.insert("output".to_string(), Message::Array(Arc::new(items)));
                out.insert("count".to_string(), Message::Integer(count as i64));
                return Ok(out);
            }
        }
    }

    // Not enough items yet — no output
    Ok(HashMap::new())
}

// ── Passthrough (identity / debug tap) ──────────────────────────

/// Passes data through unchanged. Useful for debugging and tap points.
#[actor(PassthroughActor, inports::<10>(input), outports::<1>(output), state(MemoryState))]
pub async fn passthrough_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let input = payload.get("input").cloned().unwrap_or(Message::Flow);
    Ok([("output".to_string(), input)].into())
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
