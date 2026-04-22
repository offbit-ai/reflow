//! Voronoi diagram generator.
//!
//! Outputs a 2D Voronoi diagram as f64 grid (distance to nearest cell point)
//! plus cell ID grid for segmentation.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::json;
use std::collections::HashMap;

#[actor(
    VoronoiActor,
    inports::<10>(seed),
    outports::<1>(distance, cell_id, metadata),
    state(MemoryState)
)]
pub async fn voronoi_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let c = ctx.get_config_hashmap();
    let p = ctx.get_payload();

    let width = c.get("width").and_then(|v| v.as_u64()).unwrap_or(256) as usize;
    let height = c.get("height").and_then(|v| v.as_u64()).unwrap_or(256) as usize;
    let cell_count = c.get("cellCount").and_then(|v| v.as_u64()).unwrap_or(32) as usize;
    let seed = match p.get("seed") {
        Some(Message::Float(v)) => *v,
        _ => c.get("seed").and_then(|v| v.as_f64()).unwrap_or(0.0),
    };
    let mode = c
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("distance")
        .to_string();

    // Generate random cell points using hash
    let mut points = Vec::with_capacity(cell_count);
    for i in 0..cell_count {
        let px = reflow_sdf::noise::value_noise(i as f64 * 7.13 + seed, 0.0, 0.0);
        let py = reflow_sdf::noise::value_noise(0.0, i as f64 * 11.37 + seed, 0.0);
        points.push((px, py));
    }

    let mut distance_grid = vec![0.0f64; width * height];
    let mut id_grid = vec![0u32; width * height];

    for y in 0..height {
        for x in 0..width {
            let px = x as f64 / width as f64;
            let py = y as f64 / height as f64;

            let mut min_dist = f64::MAX;
            let mut min_id = 0u32;

            for (i, &(cx, cy)) in points.iter().enumerate() {
                let dx = px - cx;
                let dy = py - cy;
                let d = (dx * dx + dy * dy).sqrt();
                if d < min_dist {
                    min_dist = d;
                    min_id = i as u32;
                }
            }

            let idx = y * width + x;
            distance_grid[idx] = min_dist;
            id_grid[idx] = min_id;
        }
    }

    let dist_bytes: Vec<u8> = distance_grid.iter().flat_map(|v| v.to_le_bytes()).collect();
    let id_bytes: Vec<u8> = id_grid.iter().flat_map(|v| v.to_le_bytes()).collect();

    let mut out = HashMap::new();
    out.insert(
        match mode.as_str() {
            "cell_id" => "cell_id",
            _ => "distance",
        }
        .to_string(),
        Message::bytes(dist_bytes),
    );
    out.insert("cell_id".to_string(), Message::bytes(id_bytes));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width, "height": height,
            "cellCount": cell_count,
            "dataType": "f64",
        }))),
    );
    Ok(out)
}
