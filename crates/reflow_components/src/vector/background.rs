//! Background actor — generates solid or gradient RGBA buffer.
//!
//! ## Config
//!
//! ```json
//! { "type": "solid", "color": [20, 10, 50, 255], "width": 1280, "height": 720 }
//! { "type": "linearGradient", "from": [10, 5, 40, 255], "to": [30, 15, 80, 255],
//!   "angle": 90, "width": 1280, "height": 720 }
//! { "type": "radialGradient", "from": [50, 30, 100, 255], "to": [10, 5, 40, 255],
//!   "center": [0.5, 0.5], "radius": 0.8, "width": 1280, "height": 720 }
//! ```
//!
//! ## Inports
//! - `tick` — triggers regeneration (for animated backgrounds)
//! - `params` — override config dynamically
//!
//! ## Outports
//! - `image` — RGBA bytes (width * height * 4)

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    BackgroundActor,
    inports::<10>(tick, params),
    outports::<1>(image, metadata),
    state(MemoryState)
)]
pub async fn background_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let mut params = config.clone();
    if let Some(Message::Object(obj)) = payload.get("params") {
        let v: Value = obj.as_ref().clone().into();
        if let Value::Object(map) = v {
            for (k, v) in map {
                params.insert(k, v);
            }
        }
    }

    let width = params.get("width").and_then(|v| v.as_u64()).unwrap_or(1280) as usize;
    let height = params.get("height").and_then(|v| v.as_u64()).unwrap_or(720) as usize;
    let bg_type = params
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("solid");

    let mut buf = vec![0u8; width * height * 4];

    match bg_type {
        "linearGradient" => {
            let from = parse_rgba(&params, "from", [0, 0, 0, 255]);
            let to = parse_rgba(&params, "to", [255, 255, 255, 255]);
            let angle_deg = params.get("angle").and_then(|v| v.as_f64()).unwrap_or(90.0);
            let angle = angle_deg.to_radians();
            let cos_a = angle.cos();
            let sin_a = angle.sin();

            for y in 0..height {
                for x in 0..width {
                    let nx = x as f64 / width as f64 - 0.5;
                    let ny = y as f64 / height as f64 - 0.5;
                    let t = (nx * cos_a + ny * sin_a + 0.5).clamp(0.0, 1.0);
                    let off = (y * width + x) * 4;
                    for c in 0..4 {
                        buf[off + c] = (from[c] as f64 + (to[c] as f64 - from[c] as f64) * t) as u8;
                    }
                }
            }
        }
        "radialGradient" => {
            let from = parse_rgba(&params, "from", [255, 255, 255, 255]);
            let to = parse_rgba(&params, "to", [0, 0, 0, 255]);
            let center = params
                .get("center")
                .and_then(|v| v.as_array())
                .map(|a| {
                    [
                        a.first().and_then(|v| v.as_f64()).unwrap_or(0.5),
                        a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.5),
                    ]
                })
                .unwrap_or([0.5, 0.5]);
            let radius = params.get("radius").and_then(|v| v.as_f64()).unwrap_or(0.8);

            for y in 0..height {
                for x in 0..width {
                    let nx = x as f64 / width as f64 - center[0];
                    let ny = y as f64 / height as f64 - center[1];
                    let dist = (nx * nx + ny * ny).sqrt() / radius;
                    let t = dist.clamp(0.0, 1.0);
                    let off = (y * width + x) * 4;
                    for c in 0..4 {
                        buf[off + c] = (from[c] as f64 + (to[c] as f64 - from[c] as f64) * t) as u8;
                    }
                }
            }
        }
        _ => {
            // Solid
            let color = parse_rgba(&params, "color", [0, 0, 0, 255]);
            for i in 0..(width * height) {
                buf[i * 4] = color[0];
                buf[i * 4 + 1] = color[1];
                buf[i * 4 + 2] = color[2];
                buf[i * 4 + 3] = color[3];
            }
        }
    }

    let mut out = HashMap::new();
    out.insert("image".to_string(), Message::bytes(buf));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width,
            "height": height,
            "type": bg_type,
        }))),
    );
    Ok(out)
}

fn parse_rgba(params: &HashMap<String, Value>, key: &str, default: [u8; 4]) -> [u8; 4] {
    params
        .get(key)
        .and_then(|v| v.as_array())
        .map(|a| {
            [
                a.first()
                    .and_then(|v| v.as_u64())
                    .unwrap_or(default[0] as u64) as u8,
                a.get(1)
                    .and_then(|v| v.as_u64())
                    .unwrap_or(default[1] as u64) as u8,
                a.get(2)
                    .and_then(|v| v.as_u64())
                    .unwrap_or(default[2] as u64) as u8,
                a.get(3)
                    .and_then(|v| v.as_u64())
                    .unwrap_or(default[3] as u64) as u8,
            ]
        })
        .unwrap_or(default)
}
