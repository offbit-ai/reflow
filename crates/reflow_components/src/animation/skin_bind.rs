//! Skin bind actor — associates per-vertex bone weights with a mesh.
//!
//! If no explicit weights are provided, auto-assigns by nearest-bone
//! distance from the skeleton's bind-pose bone positions.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    SkinBindActor,
    inports::<10>(mesh, skeleton, weights),
    outports::<1>(skin, skinned_mesh, metadata),
    state(MemoryState),
    await_inports(mesh, skeleton)
)]
pub async fn skin_bind_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let max_influences = config
        .get("maxInfluences")
        .and_then(|v| v.as_u64())
        .unwrap_or(4) as usize;
    let stride = config.get("stride").and_then(|v| v.as_u64()).unwrap_or(24) as usize;

    // Cache inputs as they arrive
    if let Some(Message::Bytes(b)) = payload.get("mesh") {
        let encoded = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(&**b)
        };
        ctx.pool_upsert("_cache", "mesh_b64", json!(encoded));
    }
    if let Some(Message::Object(obj)) = payload.get("skeleton") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_cache", "skeleton", v);
    }
    if let Some(Message::Bytes(b)) = payload.get("weights") {
        let encoded = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(&**b)
        };
        ctx.pool_upsert("_cache", "weights_b64", json!(encoded));
    }

    // Retrieve required inputs (framework guarantees mesh + skeleton are present via await_inports)
    let cache: HashMap<String, Value> = ctx.get_pool("_cache").into_iter().collect();
    let mesh_bytes = {
        use base64::Engine;
        let s = cache.get("mesh_b64").and_then(|v| v.as_str()).unwrap();
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .unwrap_or_default()
    };
    let skeleton: Value = cache.get("skeleton").cloned().unwrap();

    let vertex_count = mesh_bytes.len() / stride;

    let bones = skeleton
        .get("bones")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("Skeleton missing bones array"))?;

    // Parse or auto-generate weights
    let weights_data = match cache.get("weights_b64").and_then(|v| v.as_str()) {
        Some(s) => {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(s)
                .unwrap_or_default()
        }
        None => {
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

/// Auto-assign bone weights by distance to bone segments.
///
/// For each vertex, computes the distance to every bone's line segment
/// (from parent joint to child joint in world space), not just the bone
/// head. This gives smooth weight falloff along the bone's length and
/// prevents mesh tearing at segment boundaries.
fn auto_assign_weights(
    mesh_bytes: &[u8],
    stride: usize,
    bones: &[Value],
    max_influences: usize,
) -> Vec<u8> {
    use super::math_helpers::{mat4_mul, MAT4_IDENTITY};

    let vertex_count = mesh_bytes.len() / stride;
    let entry_size = 2 + 4; // u16 + f32
    let mut out = Vec::with_capacity(vertex_count * max_influences * entry_size);
    let bone_count = bones.len();

    // Parse parent indices and local bind transforms
    let mut parents: Vec<i32> = Vec::with_capacity(bone_count);
    let mut local_mats: Vec<[f32; 16]> = Vec::with_capacity(bone_count);

    for b in bones {
        parents.push(b.get("parent").and_then(|v| v.as_i64()).unwrap_or(-1) as i32);
        let m = if let Some(arr) = b.get("localBindTransform").and_then(|v| v.as_array()) {
            let mut mat = [0.0f32; 16];
            for (i, v) in arr.iter().enumerate().take(16) {
                mat[i] = v.as_f64().unwrap_or(0.0) as f32;
            }
            mat
        } else {
            MAT4_IDENTITY
        };
        local_mats.push(m);
    }

    // Compute world positions by walking hierarchy: world = parent_world * local
    let mut world_positions: Vec<[f32; 3]> = vec![[0.0; 3]; bone_count];
    let mut world_mats: Vec<[f32; 16]> = vec![MAT4_IDENTITY; bone_count];
    for i in 0..bone_count {
        let p = parents[i];
        if p >= 0 && (p as usize) < bone_count {
            world_mats[i] = mat4_mul(&world_mats[p as usize], &local_mats[i]);
        } else {
            world_mats[i] = local_mats[i];
        }
        // World position = translation column of world matrix
        world_positions[i] = [world_mats[i][12], world_mats[i][13], world_mats[i][14]];
    }

    // Build bone segments: each bone defines a segment from parent_pos to bone_pos.
    // Root bone (no parent) uses a zero-length segment at its position.
    let segments: Vec<([f32; 3], [f32; 3])> = (0..bone_count)
        .map(|i| {
            let p = parents[i];
            let bone_pos = world_positions[i];
            let parent_pos = if p >= 0 && (p as usize) < bone_count {
                world_positions[p as usize]
            } else {
                bone_pos // root: zero-length segment
            };
            (parent_pos, bone_pos)
        })
        .collect();

    for i in 0..vertex_count {
        let off = i * stride;
        let vx = f32::from_le_bytes(mesh_bytes[off..off + 4].try_into().unwrap());
        let vy = f32::from_le_bytes(mesh_bytes[off + 4..off + 8].try_into().unwrap());
        let vz = f32::from_le_bytes(mesh_bytes[off + 8..off + 12].try_into().unwrap());
        let vertex = [vx, vy, vz];

        // Distance to each bone segment
        let mut dists: Vec<(usize, f32)> = segments
            .iter()
            .enumerate()
            .map(|(bi, (seg_a, seg_b))| {
                let d = point_to_segment_distance(vertex, *seg_a, *seg_b);
                (bi, d)
            })
            .collect();
        dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        // Inverse-distance weighting with smoothing
        let top: Vec<(usize, f32)> = dists
            .iter()
            .take(max_influences)
            .map(|(bi, d)| (*bi, 1.0 / (d + 0.01)))
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

/// Distance from point `p` to line segment `a→b`.
fn point_to_segment_distance(p: [f32; 3], a: [f32; 3], b: [f32; 3]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let ab_len_sq = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];

    if ab_len_sq < 1e-8 {
        // Zero-length segment: just point distance
        return (ap[0] * ap[0] + ap[1] * ap[1] + ap[2] * ap[2]).sqrt();
    }

    // Project p onto the line, clamped to [0, 1]
    let t = ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / ab_len_sq).clamp(0.0, 1.0);

    let closest = [a[0] + t * ab[0], a[1] + t * ab[1], a[2] + t * ab[2]];

    let dx = p[0] - closest[0];
    let dy = p[1] - closest[1];
    let dz = p[2] - closest[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}
