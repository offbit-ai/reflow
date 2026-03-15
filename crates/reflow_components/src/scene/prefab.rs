//! Prefab actor — packages mesh + material into a reusable template.
//!
//! Takes mesh bytes and material config, wraps them as a named prefab
//! object that can be spawned multiple times via InstanceActor.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    PrefabActor,
    inports::<10>(mesh, material),
    outports::<10>(prefab, metadata),
    state(MemoryState)
)]
pub async fn prefab_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let name = config
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("prefab")
        .to_string();

    let mesh_size = match payload.get("mesh") {
        Some(Message::Bytes(b)) => b.len(),
        _ => 0,
    };

    // Extract material properties
    let material = match payload.get("material") {
        Some(Message::Object(obj)) => {
            let v: serde_json::Value = obj.as_ref().clone().into();
            v
        }
        _ => json!({
            "color": [0.8, 0.8, 0.8],
            "roughness": 0.5,
            "metallic": 0.0,
        }),
    };

    // Build prefab descriptor
    let prefab = json!({
        "id": name,
        "type": "prefab",
        "meshSize": mesh_size,
        "material": material,
        "stride": config.get("stride").and_then(|v| v.as_u64()).unwrap_or(24),
    });

    let mut out = HashMap::new();
    out.insert(
        "prefab".to_string(),
        Message::object(EncodableValue::from(prefab.clone())),
    );

    // Pass through mesh bytes alongside the prefab descriptor
    if let Some(mesh) = payload.get("mesh") {
        out.insert("prefab_mesh".to_string(), mesh.clone());
    }

    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "name": name,
            "meshSize": mesh_size,
        }))),
    );
    Ok(out)
}
