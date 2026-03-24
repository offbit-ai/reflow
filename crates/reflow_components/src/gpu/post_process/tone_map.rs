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

    for i in 0..pixel_count {
        let off = i * 4;
        let mut r = output[off] as f32 / 255.0 * exposure;
        let mut g = output[off + 1] as f32 / 255.0 * exposure;
        let mut b = output[off + 2] as f32 / 255.0 * exposure;

        // Tone map
        match mode {
            "aces" => {
                r = aces_filmic(r);
                g = aces_filmic(g);
                b = aces_filmic(b);
            }
            "reinhard" => {
                r = r / (1.0 + r);
                g = g / (1.0 + g);
                b = b / (1.0 + b);
            }
            "uncharted2" => {
                r = uncharted2(r);
                g = uncharted2(g);
                b = uncharted2(b);
            }
            _ => {
                r = r.min(1.0);
                g = g.min(1.0);
                b = b.min(1.0);
            }
        }

        // Gamma correction
        r = r.powf(inv_gamma);
        g = g.powf(inv_gamma);
        b = b.powf(inv_gamma);

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
