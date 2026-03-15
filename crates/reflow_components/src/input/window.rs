//! Window/viewport event actor.
//!
//! Outputs window events (resize, focus, visibility, orientation).
//! Config (injected by runtime on each event):
//!   type (resize/focus/blur/visibilitychange/orientationchange),
//!   width, height, devicePixelRatio, orientation, hidden

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    WindowEventActor,
    inports::<1>(),
    outports::<50>(event, size, dpr),
    state(MemoryState)
)]
pub async fn window_event_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let event_type = config
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("resize")
        .to_string();
    let width = config.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let height = config.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let dpr = config
        .get("devicePixelRatio")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let orientation = config
        .get("orientation")
        .and_then(|v| v.as_str())
        .unwrap_or("landscape")
        .to_string();
    let hidden = config
        .get("hidden")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut out = HashMap::new();
    out.insert(
        "event".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": event_type,
            "width": width,
            "height": height,
            "devicePixelRatio": dpr,
            "orientation": orientation,
            "hidden": hidden,
        }))),
    );
    out.insert(
        "size".to_string(),
        Message::object(EncodableValue::from(json!({ "x": width, "y": height }))),
    );
    out.insert("dpr".to_string(), Message::Float(dpr));
    Ok(out)
}
