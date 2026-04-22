//! Screen-Space Ambient Occlusion (SSAO).
//!
//! Approximates ambient occlusion from the depth buffer using hemisphere
//! sampling. Darkens creases and occluded areas.
//!
//! Note: This is a simplified CPU implementation. For production quality,
//! use a GPU compute pass with proper depth buffer access (HBAO/GTAO).
//!
//! ## Config
//! - `radius` — sample radius in pixels (default 4)
//! - `intensity` — darkness strength (default 1.0)
//! - `bias` — depth bias to avoid self-occlusion (default 0.025)
//!
//! ## Inports
//! - `input` — RGBA8 pixel bytes
//! - `width`, `height` — frame dimensions
//!
//! ## Outports
//! - `output` — RGBA8 with AO applied

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::ActorContext;
use serde_json::json;
use std::collections::HashMap;

#[actor(
    SSAOActor,
    inports::<10>(input, width, height),
    outports::<1>(output),
    state(MemoryState),
    await_inports(input)
)]
pub async fn ssao_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let radius = config.get("radius").and_then(|v| v.as_u64()).unwrap_or(4) as i32;
    let intensity = config
        .get("intensity")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;

    if let Some(Message::Integer(w)) = payload.get("width") {
        ctx.pool_upsert("_ssao", "w", json!(w));
    }
    if let Some(Message::Integer(h)) = payload.get("height") {
        ctx.pool_upsert("_ssao", "h", json!(h));
    }

    let pool: HashMap<String, serde_json::Value> = ctx.get_pool("_ssao").into_iter().collect();
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

    // Approximate depth from luminance (no real depth buffer in CPU path)
    let lum = |off: usize| -> f32 {
        let r = pixels[off] as f32 / 255.0;
        let g = pixels[off + 1] as f32 / 255.0;
        let b = pixels[off + 2] as f32 / 255.0;
        r * 0.2126 + g * 0.7152 + b * 0.0722
    };

    // Compute AO per pixel using neighbor luminance comparison
    let mut ao_map = vec![1.0f32; w * h];
    for y in radius..(h as i32 - radius) {
        for x in radius..(w as i32 - radius) {
            let center_lum = lum(((y as usize) * w + x as usize) * 4);
            let mut occlusion = 0.0f32;
            let mut samples = 0.0f32;

            // Sample in a cross pattern for speed
            for &(dx, dy) in &[
                (radius, 0),
                (-radius, 0),
                (0, radius),
                (0, -radius),
                (radius, radius),
                (-radius, -radius),
                (radius, -radius),
                (-radius, radius),
            ] {
                let nx = x + dx;
                let ny = y + dy;
                let sample_lum = lum((ny as usize * w + nx as usize) * 4);
                let diff = center_lum - sample_lum;
                if diff > 0.01 {
                    occlusion += diff.min(0.3);
                }
                samples += 1.0;
            }

            let ao = 1.0 - (occlusion / samples) * intensity;
            ao_map[y as usize * w + x as usize] = ao.clamp(0.3, 1.0);
        }
    }

    // Apply AO
    let mut output = pixels;
    for y in 0..h {
        for x in 0..w {
            let off = (y * w + x) * 4;
            let ao = ao_map[y * w + x];
            output[off] = (output[off] as f32 * ao) as u8;
            output[off + 1] = (output[off + 1] as f32 * ao) as u8;
            output[off + 2] = (output[off + 2] as f32 * ao) as u8;
        }
    }

    let mut out = HashMap::new();
    out.insert("output".to_string(), Message::bytes(output));
    Ok(out)
}
