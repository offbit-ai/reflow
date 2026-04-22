//! STL mesh export actor.
//!
//! Takes mesh bytes (pos3+normal3 f32 per vertex) and converts to
//! binary STL format as Message::Bytes.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::json;
use std::collections::HashMap;

#[actor(
    StlExportActor,
    inports::<10>(mesh),
    outports::<1>(output, metadata, error),
    state(MemoryState)
)]
pub async fn stl_export_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let bytes = match payload.get("mesh") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on mesh port")),
    };

    let stride = config.get("stride").and_then(|v| v.as_u64()).unwrap_or(24) as usize;
    let floats_per_vertex = stride / 4;
    let has_normals = floats_per_vertex >= 6;

    let float_data: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    let vertex_count = float_data.len() / floats_per_vertex;
    let triangle_count = vertex_count / 3;

    // Binary STL format:
    // 80 bytes header + 4 bytes triangle count + N × (12 normal + 36 vertices + 2 attribute) = 50 bytes/tri
    let stl_size = 80 + 4 + triangle_count * 50;
    let mut stl = Vec::with_capacity(stl_size);

    // Header (80 bytes)
    let mut header = [0u8; 80];
    let label = b"Reflow SDF Export";
    header[..label.len()].copy_from_slice(label);
    stl.extend_from_slice(&header);

    // Triangle count
    stl.extend_from_slice(&(triangle_count as u32).to_le_bytes());

    // Triangles
    for tri in 0..triangle_count {
        let i0 = tri * 3;
        let i1 = i0 + 1;
        let i2 = i0 + 2;

        let p = |vi: usize| -> [f32; 3] {
            let b = vi * floats_per_vertex;
            [float_data[b], float_data[b + 1], float_data[b + 2]]
        };

        let p0 = p(i0);
        let p1 = p(i1);
        let p2 = p(i2);

        // Normal: use vertex normal average if available, else compute face normal
        let normal = if has_normals {
            let n = |vi: usize| -> [f32; 3] {
                let b = vi * floats_per_vertex + 3;
                [float_data[b], float_data[b + 1], float_data[b + 2]]
            };
            let n0 = n(i0);
            let n1 = n(i1);
            let n2 = n(i2);
            let avg = [
                (n0[0] + n1[0] + n2[0]) / 3.0,
                (n0[1] + n1[1] + n2[1]) / 3.0,
                (n0[2] + n1[2] + n2[2]) / 3.0,
            ];
            let len = (avg[0] * avg[0] + avg[1] * avg[1] + avg[2] * avg[2]).sqrt();
            if len > 1e-6 {
                [avg[0] / len, avg[1] / len, avg[2] / len]
            } else {
                [0.0, 1.0, 0.0]
            }
        } else {
            // Face normal from cross product
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 1e-6 {
                [n[0] / len, n[1] / len, n[2] / len]
            } else {
                [0.0, 1.0, 0.0]
            }
        };

        // Normal (12 bytes)
        for v in &normal {
            stl.extend_from_slice(&v.to_le_bytes());
        }
        // Vertex 1, 2, 3 (36 bytes)
        for v in &p0 {
            stl.extend_from_slice(&v.to_le_bytes());
        }
        for v in &p1 {
            stl.extend_from_slice(&v.to_le_bytes());
        }
        for v in &p2 {
            stl.extend_from_slice(&v.to_le_bytes());
        }
        // Attribute byte count (2 bytes)
        stl.extend_from_slice(&0u16.to_le_bytes());
    }

    let mut results = HashMap::new();
    results.insert("output".to_string(), Message::bytes(stl));
    results.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "format": "stl",
            "triangleCount": triangle_count,
            "size": stl_size,
        }))),
    );
    Ok(results)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
