//! Terrain actor — combines heightmap mesh + texture into a scene object.
//!
//! Takes mesh bytes (from HeightmapToMesh), optional heightmap data
//! (from NoiseGenerator), and optional texture. Outputs a terrain scene
//! object for the scene graph pool with embedded heightmap statistics.
//!
//! Uses state accumulation: fires on each input, only emits output once
//! both `mesh` and `heightmap` have arrived.

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
    state(MemoryState),
    await_inports(mesh, heightmap)
)]
pub async fn terrain_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    // Accumulate inputs into the pool as they arrive
    if let Some(Message::Bytes(b)) = payload.get("mesh") {
        ctx.pool_upsert("_state", "mesh_size", json!(b.len()));
    }
    if payload.get("texture").is_some() {
        ctx.pool_upsert("_state", "has_texture", json!(true));
    }
    if let Some(Message::Bytes(b)) = payload.get("heightmap") {
        // Parse heightmap stats
        let sample_count = b.len() / 8;
        let grid_size = (sample_count as f64).sqrt() as usize;
        let mut min_h = f64::MAX;
        let mut max_h = f64::MIN;
        let mut sum = 0.0;
        let mut count = 0usize;

        for i in 0..sample_count {
            let off = i * 8;
            if off + 8 <= b.len() {
                let h = f64::from_le_bytes(b[off..off + 8].try_into().unwrap());
                if h < min_h { min_h = h; }
                if h > max_h { max_h = h; }
                sum += h;
                count += 1;
            }
        }

        let avg = if count > 0 { sum / count as f64 } else { 0.0 };

        ctx.pool_upsert("_state", "heightmap", json!({
            "resolution": grid_size,
            "samples": sample_count,
            "minHeight": (min_h * 1000.0).round() / 1000.0,
            "maxHeight": (max_h * 1000.0).round() / 1000.0,
            "avgHeight": (avg * 1000.0).round() / 1000.0,
        }));
    }

    // Retrieve state (mesh + heightmap guaranteed present via await_inports)
    let state: HashMap<String, serde_json::Value> = ctx.get_pool("_state").into_iter().collect();

    let id = config
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("terrain")
        .to_string();

    let mesh_size = state.get("mesh_size")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let has_texture = state.contains_key("has_texture");
    let heightmap_info = state.get("heightmap").cloned();

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

    let mut terrain_data = json!({
        "width": width,
        "depth": depth,
        "heightScale": height_scale,
        "meshSize": mesh_size,
        "hasTexture": has_texture,
        "hasHeightmap": true,
    });

    if let Some(hm) = &heightmap_info {
        terrain_data.as_object_mut().unwrap().insert("heightmap".to_string(), hm.clone());
    }

    let object = json!({
        "id": id,
        "type": "terrain",
        "transform": {
            "position": pos,
            "rotation": [0.0, 0.0, 0.0],
            "scale": [1.0, 1.0, 1.0],
        },
        "terrain": terrain_data,
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
            "hasHeightmap": true,
        }))),
    );
    Ok(out)
}
