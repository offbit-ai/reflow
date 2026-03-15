//! Gamepad input actor.
//!
//! Outputs gamepad state when triggered by the runtime.
//! Config (injected by runtime on each poll/event):
//!   index, id, connected, axes[], buttons[],
//!   timestamp, mapping

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    GamepadInputActor,
    inports::<1>(),
    outports::<50>(event, axes, buttons, sticks),
    state(MemoryState)
)]
pub async fn gamepad_input_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let event_type = config
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("gamepadstate")
        .to_string();
    let index = config.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
    let id = config
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let connected = config
        .get("connected")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Axes: typically [leftStickX, leftStickY, rightStickX, rightStickY]
    let axes: Vec<f64> = config
        .get("axes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_f64())
                .collect()
        })
        .unwrap_or_default();

    // Buttons: array of { pressed, value, touched }
    let buttons_raw = config
        .get("buttons")
        .cloned()
        .unwrap_or(json!([]));

    let mut out = HashMap::new();
    out.insert(
        "event".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": event_type,
            "index": index,
            "id": id,
            "connected": connected,
            "axes": axes,
            "buttons": buttons_raw,
        }))),
    );

    // Axes as typed array
    let axes_encoded: Vec<EncodableValue> = axes
        .iter()
        .map(|&v| EncodableValue::from(json!(v)))
        .collect();
    out.insert("axes".to_string(), Message::Array(Arc::new(axes_encoded)));

    out.insert("buttons".to_string(), Message::object(EncodableValue::from(buttons_raw)));

    // Convenience: left/right stick as vec2
    let left_x = axes.first().copied().unwrap_or(0.0);
    let left_y = axes.get(1).copied().unwrap_or(0.0);
    let right_x = axes.get(2).copied().unwrap_or(0.0);
    let right_y = axes.get(3).copied().unwrap_or(0.0);
    out.insert(
        "sticks".to_string(),
        Message::object(EncodableValue::from(json!({
            "left": { "x": left_x, "y": left_y },
            "right": { "x": right_x, "y": right_y },
        }))),
    );

    Ok(out)
}
