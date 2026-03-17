//! Canvas2D actor — composites multiple RGBA layers per frame.
//!
//! Each layer is a named inport. Config defines z-order, blend mode,
//! and opacity per layer. The actor pools all layers and composites
//! when every expected layer has arrived for the current frame.
//!
//! ## Config
//!
//! ```json
//! {
//!   "width": 640,
//!   "height": 360,
//!   "background": [8, 4, 25, 255],
//!   "layers": [
//!     { "name": "bg", "blend": "normal", "opacity": 1.0 },
//!     { "name": "glow", "blend": "screen", "opacity": 0.5 },
//!     { "name": "star", "blend": "normal", "opacity": 1.0 },
//!     { "name": "circle", "blend": "add", "opacity": 0.7 }
//!   ]
//! }
//! ```
//!
//! ## DAG wiring
//!
//! ```text
//! Background    → bg     ─┐
//! GaussianBlur  → glow   ─┤
//! StarRender    → star   ─┼→ Canvas2D → frame → collector
//! CircleRender  → circle ─┘
//! ```
//!
//! The actor fires and composites when ALL configured layers have
//! image data (pooled across invocations).

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    Canvas2DActor,
    inports::<100>(tick, layer_0, layer_1, layer_2, layer_3, layer_4, layer_5, layer_6, layer_7),
    outports::<1>(frame, metadata),
    state(MemoryState)
)]
pub async fn canvas_2d_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(640) as usize;
    let height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(360) as usize;
    let pixel_count = width * height * 4;

    let bg_color = config.get("background").and_then(|v| v.as_array()).map(|a| {
        [
            a.get(0).and_then(|v| v.as_u64()).unwrap_or(0) as u8,
            a.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as u8,
            a.get(2).and_then(|v| v.as_u64()).unwrap_or(0) as u8,
            a.get(3).and_then(|v| v.as_u64()).unwrap_or(255) as u8,
        ]
    }).unwrap_or([0, 0, 0, 255]);

    let layers_config = config.get("layers").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    // Pool every incoming image by port name
    let all_ports = ["layer_0", "layer_1", "layer_2", "layer_3", "layer_4", "layer_5", "layer_6", "layer_7"];
    for port_name in &all_ports {
        if let Some(Message::Bytes(bytes)) = payload.get(*port_name) {
            let encoded = {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.encode(&**bytes)
            };
            ctx.pool_upsert("_layers", port_name, json!(encoded));
        }
    }

    let pool: HashMap<String, Value> = ctx.get_pool("_layers").into_iter().collect();

    // Only composite when tick arrives — ensures one frame per tick.
    // Layer data arrives and is cached in pool; tick triggers the composite.
    if !payload.contains_key("tick") {
        return Ok(HashMap::new());
    }

    // ─── Composite ───

    // Start with background color
    let mut canvas = vec![0u8; pixel_count];
    for i in 0..(width * height) {
        canvas[i * 4] = bg_color[0];
        canvas[i * 4 + 1] = bg_color[1];
        canvas[i * 4 + 2] = bg_color[2];
        canvas[i * 4 + 3] = bg_color[3];
    }

    // Composite layers in config order (z-order)
    for layer_cfg in &layers_config {
        let name = match layer_cfg.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };
        let blend_mode = layer_cfg.get("blend").and_then(|v| v.as_str()).unwrap_or("normal");
        let opacity = layer_cfg.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;

        let layer_bytes = match pool.get(name).and_then(|v| v.as_str()) {
            Some(encoded) => {
                use base64::Engine;
                match base64::engine::general_purpose::STANDARD.decode(encoded) {
                    Ok(bytes) => bytes,
                    Err(_) => continue,
                }
            }
            None => continue,
        };

        if layer_bytes.len() != pixel_count {
            continue;
        }

        let mode = reflow_pixel::blend::BlendMode::from_str(blend_mode);
        reflow_pixel::blend::blend_rows(&mut canvas, &layer_bytes, mode, opacity);
    }

    // DON'T clear pool — layers stay cached. Next tick's arrivals
    // overwrite them. This prevents blank frames when layers arrive
    // at slightly different times within a tick.

    let mut out = HashMap::new();
    out.insert("frame".to_string(), Message::bytes(canvas));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width,
            "height": height,
            "layerCount": layers_config.len(),
        }))),
    );
    Ok(out)
}
