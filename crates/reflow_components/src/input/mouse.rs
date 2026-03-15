//! Mouse/pointer input actor.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    MouseInputActor,
    inports::<1>(),
    outports::<50>(event, position, button, delta),
    state(MemoryState)
)]
pub async fn mouse_input_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let e = super::extract_event_data(&ctx);

    let x = e.get("x").or(e.get("clientX")).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = e.get("y").or(e.get("clientY")).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let button = e.get("button").and_then(|v| v.as_u64()).unwrap_or(0);
    let delta_x = e.get("deltaX").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let delta_y = e.get("deltaY").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let mut out = HashMap::new();
    out.insert("event".to_string(), Message::object(EncodableValue::from(e)));
    out.insert("position".to_string(), Message::object(EncodableValue::from(json!({ "x": x, "y": y }))));
    out.insert("button".to_string(), Message::Integer(button as i64));
    out.insert("delta".to_string(), Message::object(EncodableValue::from(json!({ "x": delta_x, "y": delta_y }))));
    Ok(out)
}
