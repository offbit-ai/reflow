//! Terrain mesh generation from heightmap data.
//!
//! HeightmapToMeshActor takes a height grid (f64 LE bytes) and generates
//! a triangle mesh with positions, normals, and UVs. Output is a JSON
//! object with vertex/index arrays suitable for rendering or export.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    HeightmapToMeshActor,
    inports::<10>(input),
    outports::<1>(mesh, metadata, error),
    state(MemoryState)
)]
pub async fn heightmap_to_mesh_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let bytes = match payload.get("input") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes (f64 LE height grid) on input")),
    };

    let grid: Vec<f64> = bytes
        .chunks_exact(8)
        .map(|b| f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
        .collect();

    let total = grid.len();
    let grid_width = config
        .get("width")
        .and_then(|v| v.as_u64())
        .unwrap_or((total as f64).sqrt() as u64) as usize;
    let grid_height = total / grid_width;

    let height_scale = config
        .get("heightScale")
        .and_then(|v| v.as_f64())
        .unwrap_or(10.0) as f32;

    let mesh_width = config
        .get("meshWidth")
        .and_then(|v| v.as_f64())
        .unwrap_or(10.0) as f32;

    let mesh_depth = config
        .get("meshDepth")
        .and_then(|v| v.as_f64())
        .unwrap_or(10.0) as f32;

    // Generate vertices: position (3), normal (3), uv (2) per vertex
    let mut positions: Vec<f32> = Vec::with_capacity(grid_width * grid_height * 3);
    let mut uvs: Vec<f32> = Vec::with_capacity(grid_width * grid_height * 2);

    for gy in 0..grid_height {
        for gx in 0..grid_width {
            let u = gx as f32 / (grid_width - 1).max(1) as f32;
            let v = gy as f32 / (grid_height - 1).max(1) as f32;

            let x = (u - 0.5) * mesh_width;
            let z = (v - 0.5) * mesh_depth;
            let y = grid[gy * grid_width + gx] as f32 * height_scale;

            positions.extend_from_slice(&[x, y, z]);
            uvs.extend_from_slice(&[u, v]);
        }
    }

    // Generate indices (two triangles per quad)
    let mut indices: Vec<u32> = Vec::with_capacity((grid_width - 1) * (grid_height - 1) * 6);
    for gy in 0..(grid_height - 1) {
        for gx in 0..(grid_width - 1) {
            let tl = (gy * grid_width + gx) as u32;
            let tr = tl + 1;
            let bl = ((gy + 1) * grid_width + gx) as u32;
            let br = bl + 1;

            indices.extend_from_slice(&[tl, bl, tr]); // tri 1
            indices.extend_from_slice(&[tr, bl, br]); // tri 2
        }
    }

    // Compute normals per vertex (average of adjacent face normals)
    let mut normals = vec![0.0f32; grid_width * grid_height * 3];
    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let p0 = &positions[i0 * 3..i0 * 3 + 3];
        let p1 = &positions[i1 * 3..i1 * 3 + 3];
        let p2 = &positions[i2 * 3..i2 * 3 + 3];

        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];

        for &idx in &[i0, i1, i2] {
            normals[idx * 3] += n[0];
            normals[idx * 3 + 1] += n[1];
            normals[idx * 3 + 2] += n[2];
        }
    }

    // Normalize
    for i in 0..(grid_width * grid_height) {
        let nx = normals[i * 3];
        let ny = normals[i * 3 + 1];
        let nz = normals[i * 3 + 2];
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len > 1e-6 {
            normals[i * 3] /= len;
            normals[i * 3 + 1] /= len;
            normals[i * 3 + 2] /= len;
        } else {
            normals[i * 3 + 1] = 1.0; // default up
        }
    }

    let vertex_count = grid_width * grid_height;
    let triangle_count = indices.len() / 3;

    // Output as interleaved f32 bytes: [pos(3) + normal(3) + uv(2)] per vertex
    let stride = 8; // floats per vertex
    let mut interleaved = Vec::with_capacity(vertex_count * stride * 4);
    for i in 0..vertex_count {
        interleaved.extend_from_slice(&positions[i * 3].to_le_bytes());
        interleaved.extend_from_slice(&positions[i * 3 + 1].to_le_bytes());
        interleaved.extend_from_slice(&positions[i * 3 + 2].to_le_bytes());
        interleaved.extend_from_slice(&normals[i * 3].to_le_bytes());
        interleaved.extend_from_slice(&normals[i * 3 + 1].to_le_bytes());
        interleaved.extend_from_slice(&normals[i * 3 + 2].to_le_bytes());
        interleaved.extend_from_slice(&uvs[i * 2].to_le_bytes());
        interleaved.extend_from_slice(&uvs[i * 2 + 1].to_le_bytes());
    }

    // Index buffer as u32 LE bytes
    let index_bytes: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();

    let mut out = HashMap::new();
    out.insert(
        "mesh".to_string(),
        Message::object(EncodableValue::from(json!({
            "vertices": interleaved.len(),
            "indices": index_bytes.len(),
        }))),
    );

    // Pack both buffers into a single Bytes output: [vertices | indices]
    let mut mesh_data = interleaved;
    mesh_data.extend_from_slice(&index_bytes);
    out.insert("mesh".to_string(), Message::bytes(mesh_data));

    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "vertexCount": vertex_count,
            "triangleCount": triangle_count,
            "indexCount": indices.len(),
            "gridWidth": grid_width,
            "gridHeight": grid_height,
            "heightScale": height_scale,
            "meshWidth": mesh_width,
            "meshDepth": mesh_depth,
            "stride": stride,
            "format": "pos3_normal3_uv2_f32",
            "vertexBytes": vertex_count * stride * 4,
            "indexBytes": indices.len() * 4,
        }))),
    );

    Ok(out)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
