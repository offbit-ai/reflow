//! Window/viewport event actor.

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
pub async fn window_event_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let e = super::extract_event_data(&ctx);

    let width = e.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let height = e.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let dpr = e.get("devicePixelRatio").and_then(|v| v.as_f64()).unwrap_or(1.0);

    let mut out = HashMap::new();
    out.insert("event".to_string(), Message::object(EncodableValue::from(e)));
    out.insert("size".to_string(), Message::object(EncodableValue::from(json!({ "x": width, "y": height }))));
    out.insert("dpr".to_string(), Message::Float(dpr));
    Ok(out)
}
