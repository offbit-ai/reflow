//! Tone mapping post-process — HDR→LDR conversion.
//!
//! Applies tone mapping + gamma correction to rendered RGBA frames.
//! Operates on CPU (pixel buffer) for simplicity — GPU compute version later.
//!
//! ## Config
//! - `mode` — "aces" (default), "reinhard", "uncharted2", "exposure"
//! - `exposure` — exposure multiplier (default 1.0)
//! - `gamma` — gamma correction (default 2.2)
//!
//! ## Inports
//! - `input` — RGBA8 pixel bytes (from scene render)
//!
//! ## Outports
//! - `output` — tone-mapped RGBA8 pixel bytes

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::ActorContext;
use std::collections::HashMap;

#[actor(
    ToneMapActor,
    inports::<10>(input),
    outports::<1>(output),
    state(MemoryState),
    await_inports(input)
)]
pub async fn tone_map_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let mode = config.get("mode").and_then(|v| v.as_str()).unwrap_or("aces");
    let exposure = config.get("exposure").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let gamma = config.get("gamma").and_then(|v| v.as_f64()).unwrap_or(2.2) as f32;
    let inv_gamma = 1.0 / gamma;

    let pixels = match payload.get("input") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(HashMap::new()),
    };

    let mut output = pixels.to_vec();
    let pixel_count = output.len() / 4;
    let scale = exposure / 255.0;

    // Select tone map function pointer to avoid branch in hot loop
    let tone_fn: fn(f32) -> f32 = match mode {
        "aces" => aces_filmic,
        "reinhard" => |x: f32| x / (1.0 + x),
        "uncharted2" => uncharted2,
        _ => |x: f32| x.min(1.0),
    };

    // Process pixels — written for auto-vectorization (no branches in loop body)
    for i in 0..pixel_count {
        let off = i * 4;
        // Convert + expose + tone map + gamma in one pass
        let r = tone_fn(output[off] as f32 * scale).powf(inv_gamma);
        let g = tone_fn(output[off + 1] as f32 * scale).powf(inv_gamma);
        let b = tone_fn(output[off + 2] as f32 * scale).powf(inv_gamma);
        output[off] = (r.clamp(0.0, 1.0) * 255.0) as u8;
        output[off + 1] = (g.clamp(0.0, 1.0) * 255.0) as u8;
        output[off + 2] = (b.clamp(0.0, 1.0) * 255.0) as u8;
    }

    let mut out = HashMap::new();
    out.insert("output".to_string(), Message::bytes(output));
    Ok(out)
}

fn aces_filmic(x: f32) -> f32 {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    ((x * (a * x + b)) / (x * (c * x + d) + e)).clamp(0.0, 1.0)
}

fn uncharted2(x: f32) -> f32 {
    let a = 0.15;
    let b = 0.50;
    let c = 0.10;
    let d = 0.20;
    let e = 0.02;
    let f = 0.30;
    let w = 11.2;
    let curr = ((x * (a * x + c * b) + d * e) / (x * (a * x + b) + d * f)) - e / f;
    let white = ((w * (a * w + c * b) + d * e) / (w * (a * w + b) + d * f)) - e / f;
    curr / white
}
