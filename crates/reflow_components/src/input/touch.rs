//! Touch input actor.
//!
//! Outputs touch event data when triggered by the runtime.
//! Config (injected by runtime on each event):
//!   type (touchstart/touchmove/touchend/touchcancel),
//!   touches: [{ id, x, y, force, radiusX, radiusY }]

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
pub async fn touch_input_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let event_type = config
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("touchstart")
        .to_string();

    let touches = config
        .get("touches")
        .cloned()
        .unwrap_or(json!([]));

    let count = if let Some(arr) = touches.as_array() {
        arr.len() as i64
    } else {
        0
    };

    let mut out = HashMap::new();
    out.insert(
        "event".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": event_type,
            "touches": touches,
            "count": count,
        }))),
    );
    out.insert(
        "touches".to_string(),
        Message::object(EncodableValue::from(touches)),
    );
    out.insert("count".to_string(), Message::Integer(count));

    // Extract pressure/force from first touch (for stylus/pen support)
    let empty = json!([]);
    let touches_ref = config.get("touches").unwrap_or(&empty);
    let pressure = touches_ref
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|t| t.get("force").or(t.get("pressure")))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    out.insert("pressure".to_string(), Message::Float(pressure));

    Ok(out)
}
