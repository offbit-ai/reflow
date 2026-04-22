//! Canvas2D actor — composites multiple RGBA layers per frame (CPU fallback).
//!
//! For GPU-composited 2D rendering use `tpl_gpu_2d_render`, which is the
//! primary 2D compositor. This actor handles cases where pre-rendered RGBA
//! buffers from external sources need to be combined.
//!
//! Layers are discovered automatically from whatever is wired to the actor.
//! Any inport receiving `Bytes` (RGBA image data) is a layer. Config provides
//! static defaults; inports override them every tick.
//!
//! ## Inports
//!
//! - `tick` — triggers compositing; one frame emitted per tick
//! - `background` — optional RGBA bytes (w×h×4) as canvas base
//! - `<any>` — any other Bytes inport is pooled as a named layer
//!
//! ## Config
//!
//! ```json
//! {
//!   "width": 640,
//!   "height": 360,
//!   "background": [0, 0, 0, 255],
//!   "layers": [
//!     "bg",
//!     { "name": "glow", "blend": "screen", "opacity": 0.5 },
//!     "cursor"
//!   ]
//! }
//! ```
//!
//! `layers` is optional. When absent, all pooled layers are composited in
//! sorted port-name order with normal blend at full opacity. Each entry is
//! either a string (port name, normal/1.0) or an object with `name`,
//! optional `blend`, and optional `opacity`.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    Canvas2DActor,
    inports::<100>(tick),
    outports::<1>(frame, metadata),
    state(MemoryState)
)]
pub async fn canvas_2d_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(640) as usize;
    let height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(360) as usize;
    let pixel_count = width * height * 4;

    // Pool any non-tick Bytes inport by port name. "background" is separate.
    for (port_name, msg) in payload.iter() {
        if port_name == "tick" {
            continue;
        }
        if let Message::Bytes(bytes) = msg {
            let encoded = {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.encode(&**bytes)
            };
            if port_name == "background" {
                ctx.pool_upsert("_canvas_bg", "image", json!(encoded));
            } else {
                ctx.pool_upsert("_layers", port_name, json!(encoded));
            }
        }
    }

    if !payload.contains_key("tick") {
        return Ok(HashMap::new());
    }

    // Resolve background
    let bg_pool: HashMap<String, Value> = ctx.get_pool("_canvas_bg").into_iter().collect();
    let mut canvas: Vec<u8> = if let Some(encoded) = bg_pool.get("image").and_then(|v| v.as_str()) {
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap_or_default();
        if decoded.len() == pixel_count {
            decoded
        } else {
            make_solid_bg(&config, pixel_count)
        }
    } else {
        make_solid_bg(&config, pixel_count)
    };

    // Resolve layer order
    let pool: HashMap<String, Value> = ctx.get_pool("_layers").into_iter().collect();

    let layers_config = config
        .get("layers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let layer_order: Vec<(String, String, f32)> = if !layers_config.is_empty() {
        layers_config
            .iter()
            .filter_map(|entry| match entry {
                Value::String(name) => Some((name.clone(), "normal".to_string(), 1.0f32)),
                Value::Object(_) => {
                    let name = entry.get("name").and_then(|v| v.as_str())?.to_string();
                    let blend = entry
                        .get("blend")
                        .and_then(|v| v.as_str())
                        .unwrap_or("normal")
                        .to_string();
                    let opacity =
                        entry.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                    Some((name, blend, opacity))
                }
                _ => None,
            })
            .collect()
    } else {
        let mut keys: Vec<String> = pool.keys().cloned().collect();
        keys.sort();
        keys.into_iter()
            .map(|k| (k, "normal".to_string(), 1.0f32))
            .collect()
    };

    // Composite
    for (name, blend_mode, opacity) in &layer_order {
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
        let mode = reflow_pixel::blend::BlendMode::parse(blend_mode);
        reflow_pixel::blend::blend_rows(&mut canvas, &layer_bytes, mode, *opacity);
    }

    let layer_count = layer_order.len();
    let mut out = HashMap::new();
    out.insert("frame".to_string(), Message::bytes(canvas));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width,
            "height": height,
            "layerCount": layer_count,
        }))),
    );
    Ok(out)
}

fn make_solid_bg(config: &HashMap<String, Value>, pixel_count: usize) -> Vec<u8> {
    let bg = config
        .get("background")
        .and_then(|v| v.as_array())
        .map(|a| {
            [
                a.first().and_then(|v| v.as_u64()).unwrap_or(0) as u8,
                a.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as u8,
                a.get(2).and_then(|v| v.as_u64()).unwrap_or(0) as u8,
                a.get(3).and_then(|v| v.as_u64()).unwrap_or(255) as u8,
            ]
        })
        .unwrap_or([0, 0, 0, 255]);
    let mut buf = vec![0u8; pixel_count];
    for i in 0..(pixel_count / 4) {
        buf[i * 4] = bg[0];
        buf[i * 4 + 1] = bg[1];
        buf[i * 4 + 2] = bg[2];
        buf[i * 4 + 3] = bg[3];
    }
    buf
}
