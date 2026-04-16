//! Bloom post-process — extracts bright regions and blurs them.
//!
//! Pipeline: threshold → downsample → Gaussian blur → additive composite.
//! Operates on CPU pixel buffer.
//!
//! ## Config
//! - `threshold` — brightness threshold (default 0.8)
//! - `intensity` — bloom strength (default 0.5)
//! - `radius` — blur radius in pixels (default 8)
//!
//! ## Inports
//! - `input` — RGBA8 pixel bytes + `width`/`height` from metadata
//!
//! ## Outports
//! - `output` — composited RGBA8 pixel bytes

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::ActorContext;
use serde_json::json;
use std::collections::HashMap;

#[actor(
    BloomPostProcessActor,
    inports::<10>(input, width, height),
    outports::<1>(output),
    state(MemoryState),
    await_inports(input)
)]
pub async fn bloom_post_process_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let threshold = config
        .get("threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.8) as f32;
    let intensity = config
        .get("intensity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5) as f32;
    let radius = config.get("radius").and_then(|v| v.as_u64()).unwrap_or(8) as usize;

    // Cache width/height
    if let Some(Message::Integer(w)) = payload.get("width") {
        ctx.pool_upsert("_bloom", "w", json!(w));
    }
    if let Some(Message::Integer(h)) = payload.get("height") {
        ctx.pool_upsert("_bloom", "h", json!(h));
    }

    let pool: HashMap<String, serde_json::Value> = ctx.get_pool("_bloom").into_iter().collect();
    let w = pool.get("w").and_then(|v| v.as_u64()).unwrap_or(512) as usize;
    let h = pool.get("h").and_then(|v| v.as_u64()).unwrap_or(512) as usize;

    let pixels = match payload.get("input") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Ok(HashMap::new()),
    };

    if pixels.len() < w * h * 4 {
        let mut out = HashMap::new();
        out.insert("output".to_string(), Message::bytes(pixels));
        return Ok(out);
    }

    // Extract bright pixels (half resolution for performance)
    let hw = w / 2;
    let hh = h / 2;
    let mut bright = vec![0.0f32; hw * hh * 3];
    for y in 0..hh {
        for x in 0..hw {
            let sx = x * 2;
            let sy = y * 2;
            let off = (sy * w + sx) * 4;
            let r = pixels[off] as f32 / 255.0;
            let g = pixels[off + 1] as f32 / 255.0;
            let b = pixels[off + 2] as f32 / 255.0;
            let lum = r * 0.2126 + g * 0.7152 + b * 0.0722;
            let factor = (lum - threshold).max(0.0);
            let bi = (y * hw + x) * 3;
            bright[bi] = r * factor;
            bright[bi + 1] = g * factor;
            bright[bi + 2] = b * factor;
        }
    }

    // Simple box blur (separable — horizontal then vertical)
    let hr = radius / 2;
    let mut blurred = bright.clone();
    // Horizontal
    for y in 0..hh {
        for x in 0..hw {
            let mut sr = 0.0f32;
            let mut sg = 0.0f32;
            let mut sb = 0.0f32;
            let mut count = 0.0f32;
            for dx in -(hr as i32)..=(hr as i32) {
                let nx = x as i32 + dx;
                if nx >= 0 && (nx as usize) < hw {
                    let bi = (y * hw + nx as usize) * 3;
                    sr += bright[bi];
                    sg += bright[bi + 1];
                    sb += bright[bi + 2];
                    count += 1.0;
                }
            }
            let bi = (y * hw + x) * 3;
            blurred[bi] = sr / count;
            blurred[bi + 1] = sg / count;
            blurred[bi + 2] = sb / count;
        }
    }
    // Vertical
    let h_pass = blurred.clone();
    for y in 0..hh {
        for x in 0..hw {
            let mut sr = 0.0f32;
            let mut sg = 0.0f32;
            let mut sb = 0.0f32;
            let mut count = 0.0f32;
            for dy in -(hr as i32)..=(hr as i32) {
                let ny = y as i32 + dy;
                if ny >= 0 && (ny as usize) < hh {
                    let bi = (ny as usize * hw + x) * 3;
                    sr += h_pass[bi];
                    sg += h_pass[bi + 1];
                    sb += h_pass[bi + 2];
                    count += 1.0;
                }
            }
            let bi = (y * hw + x) * 3;
            blurred[bi] = sr / count;
            blurred[bi + 1] = sg / count;
            blurred[bi + 2] = sb / count;
        }
    }

    // Composite: original + bloom
    let mut output = pixels;
    for y in 0..h {
        for x in 0..w {
            let off = (y * w + x) * 4;
            let bx = (x * hw) / w;
            let by = (y * hh) / h;
            let bi = (by * hw + bx) * 3;
            let br = blurred[bi] * intensity;
            let bg = blurred[bi + 1] * intensity;
            let bb = blurred[bi + 2] * intensity;
            output[off] = ((output[off] as f32 / 255.0 + br).min(1.0) * 255.0) as u8;
            output[off + 1] = ((output[off + 1] as f32 / 255.0 + bg).min(1.0) * 255.0) as u8;
            output[off + 2] = ((output[off + 2] as f32 / 255.0 + bb).min(1.0) * 255.0) as u8;
        }
    }

    let mut out = HashMap::new();
    out.insert("output".to_string(), Message::bytes(output));
    Ok(out)
}
