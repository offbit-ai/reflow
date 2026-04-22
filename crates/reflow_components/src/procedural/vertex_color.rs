//! Vertex color actor — applies per-vertex colors based on UV + noise.
//!
//! Takes a mesh (pos3+normal3, 24-byte stride) and UV data (u,v per vertex),
//! plus optional noise data from NoiseGeneratorActor. Outputs a colored mesh
//! (pos3+normal3+color3, 36-byte stride) ready for SceneRenderActor.
//!
//! Config:
//!   color1: [r,g,b] — primary color
//!   color2: [r,g,b] — secondary color (blended by noise/pattern)
//!   noiseScale: f32 — how much the noise UV is scaled (default 5.0)

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    VertexColorActor,
    inports::<10>(mesh, uv, noise),
    outports::<1>(colored_mesh, metadata),
    state(MemoryState),
    await_inports(mesh, uv)
)]
pub async fn vertex_color_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    // Cache inputs
    if let Some(Message::Bytes(b)) = payload.get("mesh") {
        use base64::Engine;
        ctx.pool_upsert(
            "_cache",
            "mesh_b64",
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(&**b)),
        );
    }
    if let Some(Message::Bytes(b)) = payload.get("uv") {
        use base64::Engine;
        ctx.pool_upsert(
            "_cache",
            "uv_b64",
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(&**b)),
        );
    }
    if let Some(Message::Bytes(b)) = payload.get("noise") {
        use base64::Engine;
        ctx.pool_upsert(
            "_cache",
            "noise_b64",
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(&**b)),
        );
    }

    // Retrieve required inputs (guaranteed populated via await_inports for mesh + uv)
    let cache: HashMap<String, serde_json::Value> = ctx.get_pool("_cache").into_iter().collect();
    let mesh_bytes = {
        use base64::Engine;
        let s = cache.get("mesh_b64").and_then(|v| v.as_str()).unwrap();
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .unwrap_or_default()
    };
    let uv_bytes = {
        use base64::Engine;
        let s = cache.get("uv_b64").and_then(|v| v.as_str()).unwrap();
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .unwrap_or_default()
    };

    // Optional noise field (from NoiseGeneratorActor)
    let noise_bytes = cache.get("noise_b64").and_then(|v| v.as_str()).map(|s| {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .unwrap_or_default()
    });

    let color1 = config
        .get("color1")
        .and_then(|v| v.as_array())
        .map(|a| {
            [
                a.first().and_then(|v| v.as_f64()).unwrap_or(0.35) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.40) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.25) as f32,
            ]
        })
        .unwrap_or([0.35, 0.40, 0.25]);
    let color2 = config
        .get("color2")
        .and_then(|v| v.as_array())
        .map(|a| {
            [
                a.first().and_then(|v| v.as_f64()).unwrap_or(0.15) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.20) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.10) as f32,
            ]
        })
        .unwrap_or([0.15, 0.20, 0.10]);
    let noise_scale = config
        .get("noiseScale")
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0) as f32;
    let noise_width = config
        .get("noiseWidth")
        .and_then(|v| v.as_u64())
        .unwrap_or(64) as usize;

    let in_stride = 24; // pos3 + normal3
    let out_stride = 36; // pos3 + normal3 + color3
    let vertex_count = mesh_bytes.len() / in_stride;

    let mut output = Vec::with_capacity(vertex_count * out_stride);

    for i in 0..vertex_count {
        let mesh_off = i * in_stride;
        let uv_off = i * 8; // 2 floats

        // Copy pos + normal
        if mesh_off + in_stride > mesh_bytes.len() {
            break;
        }
        output.extend_from_slice(&mesh_bytes[mesh_off..mesh_off + in_stride]);

        // Read UV
        let (u, v) = if uv_off + 8 <= uv_bytes.len() {
            let u = f32::from_le_bytes(uv_bytes[uv_off..uv_off + 4].try_into().unwrap());
            let v = f32::from_le_bytes(uv_bytes[uv_off + 4..uv_off + 8].try_into().unwrap());
            (u, v)
        } else {
            (0.0, 0.0)
        };

        // Sample noise for natural variation
        let noise_val = if let Some(ref noise) = noise_bytes {
            let nx = ((u.abs() * noise_scale) as usize) % noise_width;
            let ny = ((v.abs() * noise_scale) as usize) % noise_width;
            let idx = (ny * noise_width + nx) * 8;
            if idx + 8 <= noise.len() {
                let val = f64::from_le_bytes(noise[idx..idx + 8].try_into().unwrap());
                ((val + 1.0) * 0.5).clamp(0.0, 1.0) as f32
            } else {
                0.5
            }
        } else {
            0.5
        };

        // Snake skin pattern from UV coordinates:
        // - Dorsal/ventral: top (v≈0.5) is darker, belly (v≈0) lighter
        // - Lateral stripes along body length
        // - Diamond pattern from combined u+v frequencies
        // - Noise breaks up regularity

        // Belly vs back: v=0 and v=1 are the bottom seam, v=0.5 is the top
        let belly_factor = (v * std::f32::consts::PI * 2.0).cos(); // -1 at top, +1 at bottom
        let belly_blend = (belly_factor * 0.5 + 0.5).clamp(0.0, 1.0); // 0=top, 1=belly

        // Lateral stripe pattern along body
        let stripe = ((u * noise_scale * 3.0).sin() * 0.5 + 0.5).clamp(0.0, 1.0);

        // Diamond/scale pattern
        let diamond = ((u * noise_scale * 8.0 + v * 12.0).sin()
            * (u * noise_scale * 8.0 - v * 12.0).sin()
            * 0.5
            + 0.5)
            .clamp(0.0, 1.0);

        // Combine: base pattern + noise variation
        let pattern = (stripe * 0.3 + diamond * 0.4 + noise_val * 0.3).clamp(0.0, 1.0);

        // Belly is lighter (color1), back has pattern between color1 and color2
        let belly_color = [color1[0] * 1.2, color1[1] * 1.1, color1[2] * 0.9];
        let back_color = [
            color1[0] * pattern + color2[0] * (1.0 - pattern),
            color1[1] * pattern + color2[1] * (1.0 - pattern),
            color1[2] * pattern + color2[2] * (1.0 - pattern),
        ];

        let color = [
            (back_color[0] * (1.0 - belly_blend * 0.4) + belly_color[0] * belly_blend * 0.4)
                .clamp(0.0, 1.0),
            (back_color[1] * (1.0 - belly_blend * 0.4) + belly_color[1] * belly_blend * 0.4)
                .clamp(0.0, 1.0),
            (back_color[2] * (1.0 - belly_blend * 0.4) + belly_color[2] * belly_blend * 0.4)
                .clamp(0.0, 1.0),
        ];

        for f in &color {
            output.extend_from_slice(&f.to_le_bytes());
        }
    }

    let mut out = HashMap::new();
    out.insert("colored_mesh".to_string(), Message::bytes(output));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "vertexCount": vertex_count,
            "stride": out_stride,
            "format": "pos3_normal3_color3_f32",
        }))),
    );
    Ok(out)
}
