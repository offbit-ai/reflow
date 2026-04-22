//! OBJ mesh export actor.
//!
//! Takes mesh bytes (pos3+normal3 f32 per vertex) and converts to
//! Wavefront OBJ text format as Message::Bytes.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::json;
use std::collections::HashMap;

#[actor(
    ObjExportActor,
    inports::<10>(mesh),
    outports::<1>(output, metadata, error),
    state(MemoryState)
)]
pub async fn obj_export_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let bytes = match payload.get("mesh") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on mesh port")),
    };

    let name = config
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("reflow_mesh");

    // Determine format from config or metadata
    let stride = config.get("stride").and_then(|v| v.as_u64()).unwrap_or(24) as usize; // default: pos3+normal3 = 6 floats × 4 bytes

    let floats_per_vertex = stride / 4;
    let has_normals = floats_per_vertex >= 6;

    // Parse vertices
    let float_data: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    let vertex_count = float_data.len() / floats_per_vertex;

    // Build OBJ text
    let mut obj = String::with_capacity(vertex_count * 60);
    obj.push_str(&format!("# Reflow SDF Mesh Export\n"));
    obj.push_str(&format!("# Vertices: {}\n", vertex_count));
    obj.push_str(&format!("# Triangles: {}\n\n", vertex_count / 3));
    obj.push_str(&format!("o {}\n\n", name));

    // Vertex positions
    for i in 0..vertex_count {
        let base = i * floats_per_vertex;
        obj.push_str(&format!(
            "v {:.6} {:.6} {:.6}\n",
            float_data[base],
            float_data[base + 1],
            float_data[base + 2]
        ));
    }

    // Vertex normals
    if has_normals {
        obj.push('\n');
        for i in 0..vertex_count {
            let base = i * floats_per_vertex + 3;
            obj.push_str(&format!(
                "vn {:.6} {:.6} {:.6}\n",
                float_data[base],
                float_data[base + 1],
                float_data[base + 2]
            ));
        }
    }

    // Faces (triangles, 1-indexed)
    obj.push_str(&format!("\ng {}\n", name));
    for i in (0..vertex_count).step_by(3) {
        let v1 = i + 1; // OBJ is 1-indexed
        let v2 = i + 2;
        let v3 = i + 3;
        if has_normals {
            obj.push_str(&format!("f {}//{} {}//{} {}//{}\n", v1, v1, v2, v2, v3, v3));
        } else {
            obj.push_str(&format!("f {} {} {}\n", v1, v2, v3));
        }
    }

    let obj_bytes = obj.into_bytes();
    let obj_size = obj_bytes.len();

    let mut results = HashMap::new();
    results.insert("output".to_string(), Message::bytes(obj_bytes));
    results.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "format": "obj",
            "vertexCount": vertex_count,
            "triangleCount": vertex_count / 3,
            "size": obj_size,
        }))),
    );
    Ok(results)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
