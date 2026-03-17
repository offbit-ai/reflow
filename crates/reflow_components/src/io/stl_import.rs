//! STL mesh import actor.
//!
//! Parses binary STL data and outputs mesh bytes in Reflow's
//! 24-byte stride (pos3+normal3 f32 per vertex) format.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    StlImportActor,
    inports::<10>(file_data),
    outports::<1>(mesh, metadata, error),
    state(MemoryState)
)]
pub async fn stl_import_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let data = match payload.get("file_data") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on file_data port")),
    };

    let smooth = config
        .get("smoothNormals")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    parse_stl(&data, smooth)
}

pub(crate) fn parse_stl(
    data: &[u8],
    smooth_normals: bool,
) -> Result<HashMap<String, Message>, Error> {
    if data.len() < 84 {
        return Ok(error_output("STL file too small"));
    }

    // Binary STL: 80-byte header + u32 triangle count + N * 50 bytes
    let tri_count =
        u32::from_le_bytes([data[80], data[81], data[82], data[83]]) as usize;
    let expected = 84 + tri_count * 50;
    if data.len() < expected {
        return Ok(error_output(&format!(
            "STL truncated: expected {} bytes, got {}",
            expected,
            data.len()
        )));
    }

    let vertex_count = tri_count * 3;
    let mut mesh = Vec::with_capacity(vertex_count * 24); // 24 bytes per vertex

    for i in 0..tri_count {
        let base = 84 + i * 50;
        let read_f32 = |off: usize| -> f32 {
            f32::from_le_bytes([
                data[base + off],
                data[base + off + 1],
                data[base + off + 2],
                data[base + off + 3],
            ])
        };

        let face_normal = [read_f32(0), read_f32(4), read_f32(8)];

        // 3 vertices, each 3 floats starting at offset 12
        for v in 0..3 {
            let voff = 12 + v * 12;
            let pos = [read_f32(voff), read_f32(voff + 4), read_f32(voff + 8)];

            // Position
            for f in &pos {
                mesh.extend_from_slice(&f.to_le_bytes());
            }
            // Normal (face normal; smooth normals handled below)
            for f in &face_normal {
                mesh.extend_from_slice(&f.to_le_bytes());
            }
        }
    }

    if smooth_normals && vertex_count > 0 {
        smooth_mesh_normals(&mut mesh);
    }

    let mut out = HashMap::new();
    out.insert("mesh".to_string(), Message::bytes(mesh));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "format": "stl",
            "triangleCount": tri_count,
            "vertexCount": vertex_count,
            "stride": 24,
        }))),
    );
    Ok(out)
}

/// Smooth normals by averaging face normals at coincident vertex positions.
fn smooth_mesh_normals(mesh: &mut [u8]) {
    let vertex_count = mesh.len() / 24;
    if vertex_count == 0 {
        return;
    }

    fn rf(mesh: &[u8], off: usize) -> f32 {
        f32::from_le_bytes([mesh[off], mesh[off + 1], mesh[off + 2], mesh[off + 3]])
    }

    // Spatial hash: accumulate normals per position
    let mut acc: HashMap<[i32; 3], [f32; 3]> = HashMap::new();
    let scale = 10000.0f32;

    for i in 0..vertex_count {
        let base = i * 24;
        let key = [
            (rf(mesh, base) * scale) as i32,
            (rf(mesh, base + 4) * scale) as i32,
            (rf(mesh, base + 8) * scale) as i32,
        ];
        let nx = rf(mesh, base + 12);
        let ny = rf(mesh, base + 16);
        let nz = rf(mesh, base + 20);
        let entry = acc.entry(key).or_insert([0.0; 3]);
        entry[0] += nx;
        entry[1] += ny;
        entry[2] += nz;
    }

    // Write averaged normals back
    for i in 0..vertex_count {
        let base = i * 24;
        let key = [
            (rf(mesh, base) * scale) as i32,
            (rf(mesh, base + 4) * scale) as i32,
            (rf(mesh, base + 8) * scale) as i32,
        ];
        if let Some(n) = acc.get(&key) {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 1e-6 {
                mesh[base + 12..base + 16].copy_from_slice(&(n[0] / len).to_le_bytes());
                mesh[base + 16..base + 20].copy_from_slice(&(n[1] / len).to_le_bytes());
                mesh[base + 20..base + 24].copy_from_slice(&(n[2] / len).to_le_bytes());
            }
        }
    }
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
