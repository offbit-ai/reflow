//! Canvas2D actor — composites N RGBA layers in z-order.
//!
//! The core 2D compositing node. Multiple sources wire into the `layer`
//! inport. Each layer is pooled with a name and z-index. The canvas
//! composites them back-to-front each tick.
//!
//! ## Inports
//!
//! - `layer` — pooled: `{ "name": "star", "z": 1, "blend": "normal", "opacity": 1.0 }`
//!             + image data as `Message::Bytes` on the `image` key, OR sent
//!             as a separate `{ "name": "star", "image": <bytes> }` object.
//! - `tick` — triggers compositing of all pooled layers
//!
//! ## Config
//!
//! ```json
//! {
//!   "width": 1280,
//!   "height": 720,
//!   "background": [0, 0, 0, 255]
//! }
//! ```
//!
//! ## DAG wiring
//!
//! ```text
//! VectorRasterize("star")  → layer ─┐
//! VectorRasterize("circle") → layer ─┤
//! Background(gradient)      → layer ─┼→ Canvas2D → frame → FrameCollector
//! GaussianBlur(glow)       → layer ─┤
//! TextRender               → layer ─┘
//!
//! IntervalTrigger ──────────→ tick ──→
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    Canvas2DActor,
    inports::<100>(layer, tick),
    outports::<1>(frame, metadata),
    state(MemoryState)
)]
pub async fn canvas_2d_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(1280) as usize;
    let height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(720) as usize;
    let bg_color = config.get("background").and_then(|v| v.as_array()).map(|a| {
        [
            a.get(0).and_then(|v| v.as_u64()).unwrap_or(0) as u8,
            a.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as u8,
            a.get(2).and_then(|v| v.as_u64()).unwrap_or(0) as u8,
            a.get(3).and_then(|v| v.as_u64()).unwrap_or(255) as u8,
        ]
    }).unwrap_or([0, 0, 0, 255]);

    let pixel_count = width * height * 4;

    // Pool incoming layers
    if let Some(Message::Object(obj)) = payload.get("layer") {
        let v: Value = obj.as_ref().clone().into();
        let name = v.get("name").and_then(|v| v.as_str()).unwrap_or("default").to_string();
        ctx.pool_upsert("_layers_meta", &name, v);
    }
    // Also pool raw image bytes if layer sends them separately
    if let Some(Message::Bytes(bytes)) = payload.get("layer") {
        // Anonymous layer — use counter
        let count: u64 = ctx.get_pool("_counter")
            .into_iter()
            .find(|(k, _)| k == "n")
            .and_then(|(_, v)| v.as_u64())
            .unwrap_or(0);
        ctx.pool_upsert("_counter", "n", json!(count + 1));
        ctx.pool_upsert("_layers_data", &format!("layer_{}", count), json!({
            "name": format!("layer_{}", count),
            "z": count,
        }));
        // Store image as base64 in pool
        let encoded = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(&**bytes)
        };
        ctx.pool_upsert("_layers_img", &format!("layer_{}", count), json!(encoded));
    }

    // Only composite on tick
    if !payload.contains_key("tick") {
        return Ok(HashMap::new());
    }

    // Collect all layers with metadata
    let layer_metas: Vec<(String, Value)> = ctx.get_pool("_layers_meta").into_iter().collect();
    let layer_imgs: Vec<(String, Value)> = ctx.get_pool("_layers_img").into_iter().collect();

    // Sort by z-index
    let mut sorted_layers: Vec<(String, f64, String, f32)> = Vec::new(); // (name, z, blend, opacity)

    for (name, meta) in &layer_metas {
        let z = meta.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let blend = meta.get("blend").and_then(|v| v.as_str()).unwrap_or("normal").to_string();
        let opacity = meta.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        sorted_layers.push((name.clone(), z, blend, opacity));
    }
    sorted_layers.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Build canvas with background
    let mut canvas = vec![0u8; pixel_count];
    for i in 0..(width * height) {
        canvas[i * 4] = bg_color[0];
        canvas[i * 4 + 1] = bg_color[1];
        canvas[i * 4 + 2] = bg_color[2];
        canvas[i * 4 + 3] = bg_color[3];
    }

    // Composite layers
    for (name, _z, blend_mode, opacity) in &sorted_layers {
        // Find image data
        let img_data = layer_imgs.iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, v)| v.as_str())
            .and_then(|encoded| {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.decode(encoded).ok()
            });

        if let Some(layer_bytes) = img_data {
            if layer_bytes.len() == pixel_count {
                let mode = reflow_pixel::blend::BlendMode::from_str(blend_mode);
                reflow_pixel::blend::blend_rows(&mut canvas, &layer_bytes, mode, *opacity);
            }
        }
    }

    let mut out = HashMap::new();
    out.insert("frame".to_string(), Message::bytes(canvas));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width,
            "height": height,
            "layerCount": sorted_layers.len(),
            "layers": sorted_layers.iter().map(|(n, z, b, o)| json!({"name": n, "z": z, "blend": b, "opacity": o})).collect::<Vec<_>>(),
        }))),
    );
    Ok(out)
}
