//! glTF 2.0 mesh export actor.
//!
//! Takes mesh bytes (pos3+normal3 f32 per vertex) and produces a
//! self-contained glTF binary (.glb) as Message::Bytes.
//!
//! GLB format: header(12) + JSON chunk + BIN chunk.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::json;
use std::collections::HashMap;

#[actor(
    GltfExportActor,
    inports::<10>(mesh),
    outports::<1>(output, metadata, error),
    state(MemoryState)
)]
pub async fn gltf_export_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let bytes = match payload.get("mesh") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on mesh port")),
    };

    let name = config
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("mesh");
    let stride = config.get("stride").and_then(|v| v.as_u64()).unwrap_or(24) as usize;
    let floats_per_vertex = stride / 4;
    let has_normals = floats_per_vertex >= 6;

    let float_data: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    let vertex_count = float_data.len() / floats_per_vertex;

    // Build interleaved buffer: positions (+ normals if available)
    let mut bin_data: Vec<u8> = Vec::new();

    // Compute bounding box
    let mut min_pos = [f32::MAX; 3];
    let mut max_pos = [f32::MIN; 3];

    for i in 0..vertex_count {
        let base = i * floats_per_vertex;
        for j in 0..3 {
            let v = float_data[base + j];
            min_pos[j] = min_pos[j].min(v);
            max_pos[j] = max_pos[j].max(v);
            bin_data.extend_from_slice(&v.to_le_bytes());
        }
    }
    let pos_byte_length = bin_data.len();

    let mut normal_byte_offset = 0;
    let mut normal_byte_length = 0;
    if has_normals {
        normal_byte_offset = bin_data.len();
        for i in 0..vertex_count {
            let base = i * floats_per_vertex + 3;
            for j in 0..3 {
                bin_data.extend_from_slice(&float_data[base + j].to_le_bytes());
            }
        }
        normal_byte_length = bin_data.len() - normal_byte_offset;
    }

    // Pad bin to 4-byte alignment
    while bin_data.len() % 4 != 0 {
        bin_data.push(0);
    }

    // Build glTF JSON
    let mut accessors = vec![json!({
        "bufferView": 0,
        "componentType": 5126, // FLOAT
        "count": vertex_count,
        "type": "VEC3",
        "min": min_pos,
        "max": max_pos,
    })];

    let mut buffer_views = vec![json!({
        "buffer": 0,
        "byteOffset": 0,
        "byteLength": pos_byte_length,
        "target": 34962, // ARRAY_BUFFER
    })];

    let mut attributes = json!({ "POSITION": 0 });

    if has_normals {
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": normal_byte_offset,
            "byteLength": normal_byte_length,
            "target": 34962,
        }));
        accessors.push(json!({
            "bufferView": 1,
            "componentType": 5126,
            "count": vertex_count,
            "type": "VEC3",
        }));
        attributes = json!({ "POSITION": 0, "NORMAL": 1 });
    }

    let gltf_json = json!({
        "asset": { "version": "2.0", "generator": "Reflow SDF" },
        "scene": 0,
        "scenes": [{ "nodes": [0] }],
        "nodes": [{ "mesh": 0, "name": name }],
        "meshes": [{
            "name": name,
            "primitives": [{
                "attributes": attributes,
                "mode": 4, // TRIANGLES
            }]
        }],
        "accessors": accessors,
        "bufferViews": buffer_views,
        "buffers": [{ "byteLength": bin_data.len() }],
    });

    let json_str = serde_json::to_string(&gltf_json).unwrap_or_default();
    let mut json_bytes = json_str.into_bytes();
    // Pad JSON to 4-byte alignment with spaces
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }

    // Build GLB
    let total_size = 12 + 8 + json_bytes.len() + 8 + bin_data.len();
    let mut glb = Vec::with_capacity(total_size);

    // GLB header
    glb.extend_from_slice(b"glTF"); // magic
    glb.extend_from_slice(&2u32.to_le_bytes()); // version
    glb.extend_from_slice(&(total_size as u32).to_le_bytes()); // length

    // JSON chunk
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes()); // chunk length
    glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes()); // chunk type: JSON
    glb.extend_from_slice(&json_bytes);

    // BIN chunk
    glb.extend_from_slice(&(bin_data.len() as u32).to_le_bytes()); // chunk length
    glb.extend_from_slice(&0x004E4942u32.to_le_bytes()); // chunk type: BIN
    glb.extend_from_slice(&bin_data);

    let mut results = HashMap::new();
    results.insert("output".to_string(), Message::bytes(glb));
    results.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "format": "glb",
            "vertexCount": vertex_count,
            "triangleCount": vertex_count / 3,
            "size": total_size,
            "hasNormals": has_normals,
        }))),
    );
    Ok(results)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
