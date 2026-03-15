//! Skin bind actor — associates per-vertex bone weights with a mesh.
//!
//! If no explicit weights are provided, auto-assigns by nearest-bone
//! distance from the skeleton's bind-pose bone positions.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::{json, Value};
use std::collections::HashMap;

use super::math_helpers::mat4_transform_point;

#[actor(
    SkinBindActor,
    inports::<10>(mesh, skeleton, weights),
    outports::<1>(skin, skinned_mesh, metadata),
    state(MemoryState),
    await_all_inports
)]
pub async fn skin_bind_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let max_influences = config
        .get("maxInfluences")
        .and_then(|v| v.as_u64())
        .unwrap_or(4) as usize;
    let stride = config
        .get("stride")
        .and_then(|v| v.as_u64())
        .unwrap_or(24) as usize;

    // Parse mesh bytes
    let mesh_bytes = match payload.get("mesh") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Err(anyhow::anyhow!("Expected mesh bytes")),
    };

    let vertex_count = mesh_bytes.len() / stride;

    // Parse skeleton
    let skeleton: Value = match payload.get("skeleton") {
        Some(Message::Object(obj)) => obj.as_ref().clone().into(),
        _ => return Err(anyhow::anyhow!("Expected skeleton on skeleton port")),
    };

    let bones = skeleton
        .get("bones")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("Skeleton missing bones array"))?;

    // Parse or auto-generate weights
    let weights_data = match payload.get("weights") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => {
            // Auto-assign: for each vertex, find closest bones
            auto_assign_weights(&mesh_bytes, stride, bones, max_influences)
        }
    };

    // Build skinned mesh descriptor
    let skin = json!({
        "vertexCount": vertex_count,
        "maxInfluences": max_influences,
        "inputStride": stride,
        "skeletonName": skeleton.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
        "boneCount": bones.len(),
    });

    let mut out = HashMap::new();
    out.insert(
        "skin".to_string(),
        Message::object(EncodableValue::from(skin)),
    );
    // Output weights as bytes: per vertex, maxInfluences * (u16 bone_index + f32 weight) = maxInfluences * 6 bytes
    out.insert("skinned_mesh".to_string(), Message::bytes(weights_data));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "vertexCount": vertex_count,
            "maxInfluences": max_influences,
            "boneCount": bones.len(),
        }))),
    );
    Ok(out)
}

/// Auto-assign bone weights by nearest bone head position.
/// Returns packed bytes: per vertex, `max_influences` entries of [u16 bone_index, f32 weight].
fn auto_assign_weights(
    mesh_bytes: &[u8],
    stride: usize,
    bones: &[Value],
    max_influences: usize,
) -> Vec<u8> {
    let vertex_count = mesh_bytes.len() / stride;
    let entry_size = 2 + 4; // u16 + f32
    let mut out = Vec::with_capacity(vertex_count * max_influences * entry_size);

    // Extract bone world positions from bind transforms
    let bone_positions: Vec<[f32; 3]> = bones
        .iter()
        .map(|b| {
            if let Some(local) = b.get("localBindTransform").and_then(|v| v.as_array()) {
                // Translation is in columns 12,13,14 of the mat4
                // But for hierarchy we'd need world positions. Approximate with local translation.
                let tx = local.get(12).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let ty = local.get(13).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let tz = local.get(14).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                [tx, ty, tz]
            } else {
                [0.0; 3]
            }
        })
        .collect();

    for i in 0..vertex_count {
        let off = i * stride;
        let vx = f32::from_le_bytes(mesh_bytes[off..off + 4].try_into().unwrap());
        let vy = f32::from_le_bytes(mesh_bytes[off + 4..off + 8].try_into().unwrap());
        let vz = f32::from_le_bytes(mesh_bytes[off + 8..off + 12].try_into().unwrap());

        // Find closest bones
        let mut dists: Vec<(usize, f32)> = bone_positions
            .iter()
            .enumerate()
            .map(|(bi, bp)| {
                let dx = vx - bp[0];
                let dy = vy - bp[1];
                let dz = vz - bp[2];
                (bi, dx * dx + dy * dy + dz * dz)
            })
            .collect();
        dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        // Take top max_influences, compute inverse-distance weights
        let top: Vec<(usize, f32)> = dists
            .iter()
            .take(max_influences)
            .map(|(bi, d2)| (*bi, 1.0 / (d2.sqrt() + 0.001)))
            .collect();

        let total: f32 = top.iter().map(|(_, w)| w).sum();

        for j in 0..max_influences {
            if j < top.len() {
                let (bone_idx, weight) = top[j];
                out.extend_from_slice(&(bone_idx as u16).to_le_bytes());
                out.extend_from_slice(&(weight / total).to_le_bytes());
            } else {
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&0.0f32.to_le_bytes());
            }
        }
    }

    out
}
