//! Terrain actor — combines heightmap mesh + texture into a scene object.
//!
//! Takes mesh bytes (from HeightmapToMesh) and optional texture,
//! outputs a terrain scene object for the scene graph pool.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    TerrainActor,
    inports::<10>(mesh, texture, heightmap),
    outports::<10>(object, metadata),
    state(MemoryState)
)]
pub async fn terrain_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let id = config
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("terrain")
        .to_string();

    let mesh_size = match payload.get("mesh") {
        Some(Message::Bytes(b)) => b.len(),
        _ => 0,
    };

    let has_texture = payload.get("texture").is_some();
    let has_heightmap = payload.get("heightmap").is_some();

    let width = config.get("width").and_then(|v| v.as_f64()).unwrap_or(100.0);
    let depth = config.get("depth").and_then(|v| v.as_f64()).unwrap_or(100.0);
    let height_scale = config
        .get("heightScale")
        .and_then(|v| v.as_f64())
        .unwrap_or(10.0);

    let pos = [
        config.get("posX").and_then(|v| v.as_f64()).unwrap_or(0.0),
        config.get("posY").and_then(|v| v.as_f64()).unwrap_or(0.0),
        config.get("posZ").and_then(|v| v.as_f64()).unwrap_or(0.0),
    ];

    let object = json!({
        "id": id,
        "type": "terrain",
        "transform": {
            "position": pos,
            "rotation": [0.0, 0.0, 0.0],
            "scale": [1.0, 1.0, 1.0],
        },
        "terrain": {
            "width": width,
            "depth": depth,
            "heightScale": height_scale,
            "meshSize": mesh_size,
            "hasTexture": has_texture,
            "hasHeightmap": has_heightmap,
        },
    });

    let mut out = HashMap::new();
    out.insert(
        "object".to_string(),
        Message::object(EncodableValue::from(object)),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "id": id,
            "meshSize": mesh_size,
            "hasTexture": has_texture,
        }))),
    );
    Ok(out)
}
