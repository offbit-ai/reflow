//! Data emit actor — emits config `data` as flow messages on trigger.
//!
//! Useful for injecting static data into the DAG as flow rather than config.
//! Supports one-shot (emit once) or per-tick (emit every trigger) modes.
//!
//! ## Config
//!
//! ```json
//! {
//!   "data": { ... },        // arbitrary JSON to emit
//!   "oneshot": true,         // emit once then stop (default: true)
//!   "port": "output"         // outport name (default: "output")
//! }
//! ```
//!
//! ## Fan-in pattern
//!
//! Multiple DataEmitActors can wire to the same downstream inport:
//!
//! ```text
//! DataEmit("shape_0", data={type:rect,...}) ──→ renderer:shapes
//! DataEmit("shape_1", data={type:circle,...}) ──→ renderer:shapes
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::Value;
use std::collections::HashMap;

#[actor(
    DataEmitActor,
    inports::<10>(trigger),
    outports::<10>(output),
    state(MemoryState)
)]
pub async fn data_emit_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let oneshot = config
        .get("oneshot")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // One-shot guard: skip if already emitted
    if oneshot && ctx.get_pool("_emit").into_iter().any(|(k, _)| k == "done") {
        return Ok(HashMap::new());
    }

    let data = match config.get("data") {
        Some(v) => v.clone(),
        None => return Ok(HashMap::new()),
    };

    if oneshot {
        ctx.pool_upsert("_emit", "done", Value::Bool(true));
    }

    let msg = Message::object(EncodableValue::from(data));
    Ok([("output".to_string(), msg)].into())
}
