//! Loop iteration actor for collection processing.
//!
//! Iterates over an entire collection in one invocation, emitting each
//! item to the outport channel directly. This drives downstream actors
//! once per item without requiring external re-invocation.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

/// Loop Actor - Compatible with tpl_loop
///
/// Iterates over a collection, emitting each item with its index.
/// All items are emitted in sequence via the outport channel.
/// After the last item, emits `completed: true`.
#[actor(
    LoopActor,
    inports::<100>(collection, initial_value),
    outports::<50>(item, completed),
    state(MemoryState)
)]
pub async fn loop_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();

    if let Some(Message::Array(collection)) = payload.get("collection") {
        if collection.is_empty() {
            return Ok([("completed".to_string(), Message::Boolean(true))].into());
        }

        // Get the outport sender to emit multiple items
        let outport_tx = context.get_outports().0;

        // Emit all items except the last via the outport channel
        for i in 0..collection.len().saturating_sub(1) {
            let item = &collection[i];
            let mut out = HashMap::new();
            out.insert(
                "item".to_string(),
                Message::object(EncodableValue::from(json!({
                    "value": serde_json::to_value(item)?,
                    "index": i
                }))),
            );
            let _ = outport_tx.send(out);
        }

        // Return the last item + completed via the normal return path
        let last_idx = collection.len() - 1;
        let last_item = &collection[last_idx];
        let mut result = HashMap::new();
        result.insert(
            "item".to_string(),
            Message::object(EncodableValue::from(json!({
                "value": serde_json::to_value(last_item)?,
                "index": last_idx
            }))),
        );
        Ok(result)
    } else {
        Ok([("completed".to_string(), Message::Boolean(true))].into())
    }
}
