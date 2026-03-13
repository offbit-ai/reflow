//! Loop iteration actor for collection processing.

use crate::{Actor, ActorBehavior, MemoryState, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

/// Loop Actor - Compatible with tpl_loop
///
/// Iterates over a collection, emitting each item with its index.
#[actor(
    LoopActor,
    inports::<100>(collection, initial_value),
    outports::<50>(item, completed),
    state(MemoryState)
)]
pub async fn loop_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    let _config = context.get_config_hashmap();
    let payload = context.get_payload();
    let state = context.get_state();

    if let Some(Message::Array(collection)) = payload.get("collection") {
        let mut state_lock = state.lock();
        let memory_state = state_lock
            .as_mut_any()
            .downcast_mut::<MemoryState>()
            .ok_or_else(|| anyhow::anyhow!("Invalid state type"))?;

        let current_index = memory_state
            .get("loop_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        if current_index < collection.len() {
            let item = &collection[current_index];
            result.insert(
                "item".to_string(),
                Message::object(EncodableValue::from(json!({
                    "value": serde_json::to_value(item)?,
                    "index": current_index
                }))),
            );

            memory_state.insert("loop_index", json!(current_index + 1));
        } else {
            result.insert("completed".to_string(), Message::Boolean(true));
            memory_state.insert("loop_index", json!(0));
        }
    } else {
        result.insert("completed".to_string(), Message::Boolean(true));
    }

    Ok(result)
}
