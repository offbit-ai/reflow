//! Touch input actor with pressure/force support.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    TouchInputActor,
    inports::<1>(),
    outports::<50>(event, touches, count, pressure),
    state(MemoryState)
)]
pub async fn touch_input_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let e = super::extract_event_data(&ctx);

    let touches = e.get("touches").cloned().unwrap_or(json!([]));
    let count = touches.as_array().map(|a| a.len() as i64).unwrap_or(0);
    let pressure = touches
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|t| t.get("force").or(t.get("pressure")))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let mut out = HashMap::new();
    out.insert("event".to_string(), Message::object(EncodableValue::from(e)));
    out.insert("touches".to_string(), Message::object(EncodableValue::from(touches)));
    out.insert("count".to_string(), Message::Integer(count));
    out.insert("pressure".to_string(), Message::Float(pressure));
    Ok(out)
}
