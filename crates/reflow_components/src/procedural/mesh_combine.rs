//! Mesh combine actor — concatenates multiple mesh byte streams into one.
//!
//! Simply appends mesh_b after mesh_a. Both must have the same stride
//! (default 24: pos3+normal3). Overlapping geometry is hidden inside
//! the combined shape — no boolean operations needed.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    MeshCombineActor,
    inports::<10>(mesh_a, mesh_b),
    outports::<1>(mesh, metadata),
    state(MemoryState),
    await_all_inports
)]
pub async fn mesh_combine_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let stride = config.get("stride").and_then(|v| v.as_u64()).unwrap_or(24) as usize;

    let mut combined = Vec::new();

    if let Some(Message::Bytes(a)) = payload.get("mesh_a") {
        combined.extend_from_slice(&a);
    }
    if let Some(Message::Bytes(b)) = payload.get("mesh_b") {
        combined.extend_from_slice(&b);
    }

    let vertex_count = combined.len() / stride;

    let mut out = HashMap::new();
    out.insert("mesh".to_string(), Message::bytes(combined));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "vertexCount": vertex_count,
            "stride": stride,
            "format": "pos3_normal3_f32",
        }))),
    );
    Ok(out)
}
