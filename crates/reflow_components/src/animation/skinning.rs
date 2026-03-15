//! CPU skinning actor — linear blend skinning.
//!
//! Caches mesh, skin descriptor, and weights in state on first receipt.
//! Fires on every `bone_transforms` input once static data is cached.
//! Outputs deformed mesh in standard 24-byte stride (pos3+normal3).

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::{json, Value};
use std::collections::HashMap;

use super::math_helpers::*;

#[actor(
    SkinningActor,
    inports::<10>(mesh, skinned_mesh, bone_transforms, skin),
    outports::<1>(deformed_mesh, metadata),
    state(MemoryState)
)]
pub async fn skinning_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let stride = config
        .get("stride")
        .and_then(|v| v.as_u64())
        .unwrap_or(24) as usize;

    // Cache static inputs on first receipt
    if let Some(Message::Bytes(b)) = payload.get("mesh") {
        ctx.pool_upsert("_cache", "mesh_b64", json!(b64_encode(&b)));
    }
    if let Some(Message::Bytes(b)) = payload.get("skinned_mesh") {
        ctx.pool_upsert("_cache", "weights_b64", json!(b64_encode(&b)));
    }
    if let Some(Message::Object(obj)) = payload.get("skin") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_cache", "skin", v);
    }

    // bone_transforms is the per-frame trigger
    let bone_bytes = match payload.get("bone_transforms") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Ok(HashMap::new()),
    };

    // Retrieve cached data
    let cache: HashMap<String, Value> = ctx.get_pool("_cache").into_iter().collect();

    let mesh_bytes = match cache.get("mesh_b64").and_then(|v| v.as_str()) {
        Some(s) => b64_decode(s),
        None => return Ok(HashMap::new()),
    };
    let skin_bytes = match cache.get("weights_b64").and_then(|v| v.as_str()) {
        Some(s) => b64_decode(s),
        None => return Ok(HashMap::new()),
    };
    let skin_info = cache.get("skin").cloned().unwrap_or(json!({"maxInfluences": 4}));

    let vertex_count = mesh_bytes.len() / stride;
    let max_influences = skin_info
        .get("maxInfluences")
        .and_then(|v| v.as_u64())
        .unwrap_or(4) as usize;
    let bone_count = bone_bytes.len() / 64;

    // Parse bone matrices
    let mut bone_matrices: Vec<[f32; 16]> = Vec::with_capacity(bone_count);
    for i in 0..bone_count {
        let off = i * 64;
        let mut m = [0.0f32; 16];
        for j in 0..16 {
            m[j] = f32::from_le_bytes(bone_bytes[off + j * 4..off + j * 4 + 4].try_into().unwrap());
        }
        bone_matrices.push(m);
    }

    let entry_size = 6; // u16 + f32
    let weights_per_vertex = max_influences * entry_size;

    // Output: 24-byte stride (pos3 + normal3)
    let mut output = Vec::with_capacity(vertex_count * 24);

    for i in 0..vertex_count {
        let mesh_off = i * stride;
        let skin_off = i * weights_per_vertex;

        let px = f32::from_le_bytes(mesh_bytes[mesh_off..mesh_off + 4].try_into().unwrap());
        let py = f32::from_le_bytes(mesh_bytes[mesh_off + 4..mesh_off + 8].try_into().unwrap());
        let pz = f32::from_le_bytes(mesh_bytes[mesh_off + 8..mesh_off + 12].try_into().unwrap());
        let nx = f32::from_le_bytes(mesh_bytes[mesh_off + 12..mesh_off + 16].try_into().unwrap());
        let ny = f32::from_le_bytes(mesh_bytes[mesh_off + 16..mesh_off + 20].try_into().unwrap());
        let nz = f32::from_le_bytes(mesh_bytes[mesh_off + 20..mesh_off + 24].try_into().unwrap());

        let pos = [px, py, pz];
        let nor = [nx, ny, nz];

        let mut blended = [0.0f32; 16];
        for j in 0..max_influences {
            let w_off = skin_off + j * entry_size;
            if w_off + entry_size > skin_bytes.len() { break; }
            let bone_idx = u16::from_le_bytes(skin_bytes[w_off..w_off + 2].try_into().unwrap()) as usize;
            let weight = f32::from_le_bytes(skin_bytes[w_off + 2..w_off + 6].try_into().unwrap());
            if weight < 1e-6 || bone_idx >= bone_count { continue; }
            let m = &bone_matrices[bone_idx];
            for k in 0..16 { blended[k] += m[k] * weight; }
        }

        let new_pos = mat4_transform_point(&blended, pos);
        let new_nor = vec3_normalize(mat4_transform_dir(&blended, nor));

        output.extend_from_slice(&new_pos[0].to_le_bytes());
        output.extend_from_slice(&new_pos[1].to_le_bytes());
        output.extend_from_slice(&new_pos[2].to_le_bytes());
        output.extend_from_slice(&new_nor[0].to_le_bytes());
        output.extend_from_slice(&new_nor[1].to_le_bytes());
        output.extend_from_slice(&new_nor[2].to_le_bytes());
    }

    let mut out = HashMap::new();
    out.insert("deformed_mesh".to_string(), Message::bytes(output));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "vertexCount": vertex_count,
            "boneCount": bone_count,
            "stride": 24,
            "format": "pos3_normal3_f32",
        }))),
    );
    Ok(out)
}

fn b64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn b64_decode(s: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).unwrap_or_default()
}
