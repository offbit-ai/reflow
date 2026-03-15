//! Texture mapping actors.
//!
//! - TriplanarTextureActor: project texture onto mesh using surface normals
//!   (works with any geometry — no UVs needed)
//! - UVTextureActor: sample texture at mesh UV coordinates

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

// ─── Helpers ─────────────────────────────────────────────────────

struct TextureData {
    pixels: Vec<u8>, // RGBA
    width: u32,
    height: u32,
}

impl TextureData {
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let img = image::load_from_memory(bytes).ok()?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        Some(Self {
            pixels: rgba.into_raw(),
            width: w,
            height: h,
        })
    }

    /// Sample texture at UV coordinates (0–1 range, wrapping).
    fn sample(&self, u: f32, v: f32) -> [f32; 3] {
        let u = u.fract().abs();
        let v = v.fract().abs();
        let x = ((u * self.width as f32) as u32).min(self.width - 1);
        let y = ((v * self.height as f32) as u32).min(self.height - 1);
        let idx = (y * self.width + x) as usize * 4;
        if idx + 2 < self.pixels.len() {
            [
                self.pixels[idx] as f32 / 255.0,
                self.pixels[idx + 1] as f32 / 255.0,
                self.pixels[idx + 2] as f32 / 255.0,
            ]
        } else {
            [1.0, 0.0, 1.0] // magenta = missing
        }
    }
}

// ─── Triplanar Texture ──────────────────────────────────────────

/// Projects a texture onto mesh vertices using triplanar mapping.
///
/// For each vertex, blends texture samples from XY, XZ, and YZ planes
/// based on the surface normal direction. No UVs required.
///
/// Input mesh: pos3+normal3 f32 (stride 24)
/// Output mesh: pos3+normal3+color3 f32 (stride 36)
#[actor(
    TriplanarTextureActor,
    inports::<10>(mesh, texture),
    outports::<1>(mesh, metadata, error),
    state(MemoryState),
    await_all_inports
)]
pub async fn triplanar_texture_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let mesh_bytes = match payload.get("mesh") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on mesh port")),
    };

    let tex_bytes = match payload.get("texture") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on texture port")),
    };

    let texture = TextureData::from_bytes(&tex_bytes)
        .ok_or_else(|| anyhow::anyhow!("Failed to decode texture image"))?;

    let scale = config.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let sharpness = config.get("sharpness").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32;
    let in_stride = config.get("stride").and_then(|v| v.as_u64()).unwrap_or(24) as usize;
    let in_floats = in_stride / 4; // 6 for pos3+normal3

    let float_data: Vec<f32> = mesh_bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    let vertex_count = float_data.len() / in_floats;
    let out_floats = in_floats + 3; // add color3

    let mut output = Vec::with_capacity(vertex_count * out_floats);

    for i in 0..vertex_count {
        let base = i * in_floats;
        let px = float_data[base];
        let py = float_data[base + 1];
        let pz = float_data[base + 2];
        let nx = if in_floats >= 6 { float_data[base + 3] } else { 0.0 };
        let ny = if in_floats >= 6 { float_data[base + 4] } else { 1.0 };
        let nz = if in_floats >= 6 { float_data[base + 5] } else { 0.0 };

        // Triplanar blend weights from normal
        let mut wx = nx.abs().powf(sharpness);
        let mut wy = ny.abs().powf(sharpness);
        let mut wz = nz.abs().powf(sharpness);
        let wsum = wx + wy + wz;
        if wsum > 0.0 {
            wx /= wsum;
            wy /= wsum;
            wz /= wsum;
        }

        // Sample texture from 3 projections
        let col_yz = texture.sample(py * scale, pz * scale); // X-facing
        let col_xz = texture.sample(px * scale, pz * scale); // Y-facing
        let col_xy = texture.sample(px * scale, py * scale); // Z-facing

        let r = col_yz[0] * wx + col_xz[0] * wy + col_xy[0] * wz;
        let g = col_yz[1] * wx + col_xz[1] * wy + col_xy[1] * wz;
        let b = col_yz[2] * wx + col_xz[2] * wy + col_xy[2] * wz;

        // Copy original vertex data
        for j in 0..in_floats {
            output.push(float_data[base + j]);
        }
        // Append color
        output.push(r);
        output.push(g);
        output.push(b);
    }

    let out_bytes: Vec<u8> = output.iter().flat_map(|v| v.to_le_bytes()).collect();
    let out_stride = out_floats * 4;

    let mut results = HashMap::new();
    results.insert("mesh".to_string(), Message::bytes(out_bytes));
    results.insert("metadata".to_string(), Message::object(EncodableValue::from(json!({
        "vertexCount": vertex_count,
        "stride": out_stride,
        "format": "pos3_normal3_color3_f32",
        "textureWidth": texture.width,
        "textureHeight": texture.height,
        "mapping": "triplanar",
    }))));
    Ok(results)
}

// ─── UV Texture ─────────────────────────────────────────────────

/// Samples a texture at mesh UV coordinates and appends vertex colors.
///
/// Input mesh: pos3+normal3+uv2 f32 (stride 32, from HeightmapToMesh)
/// Output mesh: pos3+normal3+uv2+color3 f32 (stride 44)
#[actor(
    UVTextureActor,
    inports::<10>(mesh, texture),
    outports::<1>(mesh, metadata, error),
    state(MemoryState),
    await_all_inports
)]
pub async fn uv_texture_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let mesh_bytes = match payload.get("mesh") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on mesh port")),
    };

    let tex_bytes = match payload.get("texture") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on texture port")),
    };

    let texture = TextureData::from_bytes(&tex_bytes)
        .ok_or_else(|| anyhow::anyhow!("Failed to decode texture image"))?;

    let in_stride = config.get("stride").and_then(|v| v.as_u64()).unwrap_or(32) as usize;
    let in_floats = in_stride / 4; // 8 for pos3+normal3+uv2
    let uv_offset = config.get("uvOffset").and_then(|v| v.as_u64()).unwrap_or(6) as usize;

    if in_floats < uv_offset + 2 {
        return Ok(error_output(&format!(
            "Stride {} too small for UV at offset {} (need at least {} floats)",
            in_stride, uv_offset, uv_offset + 2
        )));
    }

    let float_data: Vec<f32> = mesh_bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    let vertex_count = float_data.len() / in_floats;
    let out_floats = in_floats + 3; // append color3

    let mut output = Vec::with_capacity(vertex_count * out_floats);

    for i in 0..vertex_count {
        let base = i * in_floats;
        let u = float_data[base + uv_offset];
        let v = float_data[base + uv_offset + 1];

        let color = texture.sample(u, v);

        // Copy original vertex data
        for j in 0..in_floats {
            output.push(float_data[base + j]);
        }
        // Append color
        output.push(color[0]);
        output.push(color[1]);
        output.push(color[2]);
    }

    let out_bytes: Vec<u8> = output.iter().flat_map(|v| v.to_le_bytes()).collect();
    let out_stride = out_floats * 4;

    let mut results = HashMap::new();
    results.insert("mesh".to_string(), Message::bytes(out_bytes));
    results.insert("metadata".to_string(), Message::object(EncodableValue::from(json!({
        "vertexCount": vertex_count,
        "stride": out_stride,
        "format": format!("pos3_normal3_uv2_color3_f32"),
        "textureWidth": texture.width,
        "textureHeight": texture.height,
        "mapping": "uv",
    }))));
    Ok(results)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
