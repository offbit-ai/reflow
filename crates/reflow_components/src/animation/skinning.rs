//! CPU skinning actor — linear blend skinning.
//!
//! Takes mesh bytes + skin weights + bone transform matrices,
//! outputs deformed mesh in standard 24-byte stride (pos3+normal3).

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
    state(MemoryState),
    await_all_inports
)]
pub async fn skinning_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let stride = config
        .get("stride")
        .and_then(|v| v.as_u64())
        .unwrap_or(24) as usize;

    // Parse mesh bytes (pos3+normal3 at minimum)
    let mesh_bytes = match payload.get("mesh") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Err(anyhow::anyhow!("Expected mesh bytes")),
    };

    let vertex_count = mesh_bytes.len() / stride;

    // Parse skin weights (from SkinBindActor): per vertex, max_influences * (u16 + f32)
    let skin_bytes = match payload.get("skinned_mesh") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Err(anyhow::anyhow!("Expected skin weights")),
    };

    // Parse skin descriptor for maxInfluences
    let skin_info: Value = match payload.get("skin") {
        Some(Message::Object(obj)) => obj.as_ref().clone().into(),
        _ => json!({"maxInfluences": 4}),
    };
    let max_influences = skin_info
        .get("maxInfluences")
        .and_then(|v| v.as_u64())
        .unwrap_or(4) as usize;

    // Parse bone transform matrices (packed f32 bytes)
    let bone_bytes = match payload.get("bone_transforms") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Err(anyhow::anyhow!("Expected bone_transforms bytes")),
    };

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

    let entry_size = 2 + 4; // u16 bone_index + f32 weight
    let weights_per_vertex = max_influences * entry_size;

    // Output: 24-byte stride (pos3 + normal3)
    let mut output = Vec::with_capacity(vertex_count * 24);

    for i in 0..vertex_count {
        let mesh_off = i * stride;
        let skin_off = i * weights_per_vertex;

        // Read original position and normal
        let px = f32::from_le_bytes(mesh_bytes[mesh_off..mesh_off + 4].try_into().unwrap());
        let py = f32::from_le_bytes(mesh_bytes[mesh_off + 4..mesh_off + 8].try_into().unwrap());
        let pz = f32::from_le_bytes(mesh_bytes[mesh_off + 8..mesh_off + 12].try_into().unwrap());
        let nx = f32::from_le_bytes(mesh_bytes[mesh_off + 12..mesh_off + 16].try_into().unwrap());
        let ny = f32::from_le_bytes(mesh_bytes[mesh_off + 16..mesh_off + 20].try_into().unwrap());
        let nz = f32::from_le_bytes(mesh_bytes[mesh_off + 20..mesh_off + 24].try_into().unwrap());

        let pos = [px, py, pz];
        let nor = [nx, ny, nz];

        // Compute blended skin matrix
        let mut blended = [0.0f32; 16];
        for j in 0..max_influences {
            let w_off = skin_off + j * entry_size;
            if w_off + entry_size > skin_bytes.len() {
                break;
            }
            let bone_idx = u16::from_le_bytes(skin_bytes[w_off..w_off + 2].try_into().unwrap()) as usize;
            let weight = f32::from_le_bytes(skin_bytes[w_off + 2..w_off + 6].try_into().unwrap());

            if weight < 1e-6 || bone_idx >= bone_count {
                continue;
            }

            let m = &bone_matrices[bone_idx];
            for k in 0..16 {
                blended[k] += m[k] * weight;
            }
        }

        // Transform position and normal
        let new_pos = mat4_transform_point(&blended, pos);
        let new_nor = vec3_normalize(mat4_transform_dir(&blended, nor));

        // Write output vertex (24 bytes)
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
