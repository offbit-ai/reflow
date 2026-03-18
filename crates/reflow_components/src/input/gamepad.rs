//! Gamepad input actor.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    GamepadInputActor,
    inports::<1>(),
    outports::<50>(event, axes, buttons, sticks),
    state(MemoryState)
)]
pub async fn gamepad_input_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let e = super::extract_event_data(&ctx);

    let axes: Vec<f64> = e
        .get("axes")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
        .unwrap_or_default();

    let mut out = HashMap::new();
    out.insert(
        "event".to_string(),
        Message::object(EncodableValue::from(e.clone())),
    );

    let axes_enc: Vec<EncodableValue> = axes
        .iter()
        .map(|&v| EncodableValue::from(json!(v)))
        .collect();
    out.insert("axes".to_string(), Message::Array(Arc::new(axes_enc)));
    out.insert(
        "buttons".to_string(),
        Message::object(EncodableValue::from(
            e.get("buttons").cloned().unwrap_or(json!([])),
        )),
    );

    let lx = axes.first().copied().unwrap_or(0.0);
    let ly = axes.get(1).copied().unwrap_or(0.0);
    let rx = axes.get(2).copied().unwrap_or(0.0);
    let ry = axes.get(3).copied().unwrap_or(0.0);
    out.insert(
        "sticks".to_string(),
        Message::object(EncodableValue::from(
            json!({ "left": { "x": lx, "y": ly }, "right": { "x": rx, "y": ry } }),
        )),
    );
    Ok(out)
}
