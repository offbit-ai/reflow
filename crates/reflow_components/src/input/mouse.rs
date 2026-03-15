//! Mouse/pointer input actor.
//!
//! Outputs mouse event data when triggered by the runtime.
//! Config (injected by runtime on each event):
//!   type (mousedown/mouseup/mousemove/click/dblclick/wheel/contextmenu),
//!   x, y, clientX, clientY, button, buttons,
//!   deltaX, deltaY (for wheel), movementX, movementY

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
pub async fn mouse_input_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let event_type = config
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("mousemove")
        .to_string();
    let x = config.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = config.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let client_x = config.get("clientX").and_then(|v| v.as_f64()).unwrap_or(x);
    let client_y = config.get("clientY").and_then(|v| v.as_f64()).unwrap_or(y);
    let button = config.get("button").and_then(|v| v.as_u64()).unwrap_or(0);
    let buttons = config.get("buttons").and_then(|v| v.as_u64()).unwrap_or(0);
    let delta_x = config.get("deltaX").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let delta_y = config.get("deltaY").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let movement_x = config
        .get("movementX")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let movement_y = config
        .get("movementY")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let mut out = HashMap::new();
    out.insert(
        "event".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": event_type,
            "x": x, "y": y,
            "clientX": client_x, "clientY": client_y,
            "button": button, "buttons": buttons,
            "deltaX": delta_x, "deltaY": delta_y,
            "movementX": movement_x, "movementY": movement_y,
        }))),
    );
    out.insert(
        "position".to_string(),
        Message::object(EncodableValue::from(json!({ "x": x, "y": y }))),
    );
    out.insert("button".to_string(), Message::Integer(button as i64));
    out.insert(
        "delta".to_string(),
        Message::object(EncodableValue::from(
            json!({ "x": delta_x, "y": delta_y }),
        )),
    );
    Ok(out)
}
