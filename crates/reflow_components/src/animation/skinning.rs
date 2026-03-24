//! CPU skinning actor — linear blend skinning.
//!
//! Caches mesh, skin descriptor, and weights in state on first receipt.
//! Fires on every `bone_transforms` input once static data is cached.
//! Outputs deformed mesh in standard 24-byte stride (pos3+normal3).

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

use super::math_helpers::*;

#[actor(
    SkinningActor,
    inports::<10>(mesh, skinned_mesh, bone_transforms, skin),
    outports::<1>(deformed_mesh, metadata),
    state(MemoryState),
    await_inports(bone_transforms)
)]
pub async fn skinning_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let stride = config.get("stride").and_then(|v| v.as_u64()).unwrap_or(24) as usize;

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

    // bone_transforms is the per-frame trigger (guaranteed present via await_inports)
    let bone_bytes = match payload.get("bone_transforms") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => unreachable!("await_inports guarantees bone_transforms"),
    };

    // Retrieve cached data (mesh + skinned_mesh arrive once, cached in pool)
    let cache: HashMap<String, Value> = ctx.get_pool("_cache").into_iter().collect();

    let mesh_bytes = match cache.get("mesh_b64").and_then(|v| v.as_str()) {
        Some(s) => b64_decode(s),
        None => return Ok(HashMap::new()), // Not arrived yet
    };
    let skin_bytes = match cache.get("weights_b64").and_then(|v| v.as_str()) {
        Some(s) => b64_decode(s),
        None => return Ok(HashMap::new()), // Not arrived yet
    };
    let skin_info = cache
        .get("skin")
        .cloned()
        .unwrap_or(json!({"maxInfluences": 4}));

    let vertex_count = mesh_bytes.len() / stride;
    let max_influences = skin_info
        .get("maxInfluences")
        .and_then(|v| v.as_u64())
        .unwrap_or(4) as usize;
    let bone_count = bone_bytes.len() / 64;


    // ── Zero-cost bone matrix decode (bytemuck reinterpret, no per-byte unpacking) ──
    let bone_floats: &[f32] = bytemuck::cast_slice(&bone_bytes[..bone_count * 64]);
    let bone_matrices: Vec<&[f32]> = (0..bone_count)
        .map(|i| &bone_floats[i * 16..(i + 1) * 16])
        .collect();

    let entry_size = 6; // u16 + f32
    let weights_per_vertex = max_influences * entry_size;

    // ── Pre-allocate output buffer (avoid per-vertex extend_from_slice) ──
    let mut output = vec![0u8; vertex_count * stride];
    // Reinterpret mesh bytes as f32 slices for SIMD-friendly access
    let mesh_floats: &[f32] = bytemuck::cast_slice(&mesh_bytes[..vertex_count * stride.min(mesh_bytes.len())]);

    // ── Hot loop: SIMD-friendly vertex skinning ──
    // Written for LLVM auto-vectorization: no branches in inner loop, aligned f32 ops
    for i in 0..vertex_count {
        let mesh_f_off = i * (stride / 4); // f32 offset
        let skin_off = i * weights_per_vertex;

        // Read position + normal as f32 (zero-cost from mesh_floats)
        let px = mesh_floats[mesh_f_off];
        let py = mesh_floats[mesh_f_off + 1];
        let pz = mesh_floats[mesh_f_off + 2];
        let nx = mesh_floats[mesh_f_off + 3];
        let ny = mesh_floats[mesh_f_off + 4];
        let nz = mesh_floats[mesh_f_off + 5];

        // Blend bone matrices (auto-vectorizable: 16-wide f32 multiply-accumulate)
        let mut blended = [0.0f32; 16];
        let mut total_weight = 0.0f32;
        let mut j = 0;
        while j < max_influences {
            let w_off = skin_off + j * entry_size;
            if w_off + entry_size > skin_bytes.len() { break; }
            let bone_idx =
                u16::from_le_bytes(skin_bytes[w_off..w_off + 2].try_into().unwrap()) as usize;
            let weight =
                f32::from_le_bytes(skin_bytes[w_off + 2..w_off + 6].try_into().unwrap());
            j += 1;
            if weight < 1e-6 || bone_idx >= bone_count { continue; }
            let m = bone_matrices[bone_idx];
            // 16-wide FMA — LLVM will auto-vectorize this to 4× SIMD ops
            blended[0] += m[0] * weight;  blended[1] += m[1] * weight;
            blended[2] += m[2] * weight;  blended[3] += m[3] * weight;
            blended[4] += m[4] * weight;  blended[5] += m[5] * weight;
            blended[6] += m[6] * weight;  blended[7] += m[7] * weight;
            blended[8] += m[8] * weight;  blended[9] += m[9] * weight;
            blended[10] += m[10] * weight; blended[11] += m[11] * weight;
            blended[12] += m[12] * weight; blended[13] += m[13] * weight;
            blended[14] += m[14] * weight; blended[15] += m[15] * weight;
            total_weight += weight;
        }

        if total_weight < 1e-6 {
            blended = super::math_helpers::MAT4_IDENTITY;
        }

        // Transform position: M × [px,py,pz,1]
        let npx = blended[0]*px + blended[4]*py + blended[8]*pz + blended[12];
        let npy = blended[1]*px + blended[5]*py + blended[9]*pz + blended[13];
        let npz = blended[2]*px + blended[6]*py + blended[10]*pz + blended[14];

        // Transform normal: upper 3x3 × [nx,ny,nz]
        let tnx = blended[0]*nx + blended[4]*ny + blended[8]*nz;
        let tny = blended[1]*nx + blended[5]*ny + blended[9]*nz;
        let tnz = blended[2]*nx + blended[6]*ny + blended[10]*nz;
        let nlen = (tnx*tnx + tny*tny + tnz*tnz).sqrt().max(1e-8);
        let nnx = tnx / nlen;
        let nny = tny / nlen;
        let nnz = tnz / nlen;

        // Write output (direct f32→bytes via bytemuck, no per-component extend_from_slice)
        let out_off = i * stride;
        output[out_off..out_off + 4].copy_from_slice(&npx.to_le_bytes());
        output[out_off + 4..out_off + 8].copy_from_slice(&npy.to_le_bytes());
        output[out_off + 8..out_off + 12].copy_from_slice(&npz.to_le_bytes());
        output[out_off + 12..out_off + 16].copy_from_slice(&nnx.to_le_bytes());
        output[out_off + 16..out_off + 20].copy_from_slice(&nny.to_le_bytes());
        output[out_off + 20..out_off + 24].copy_from_slice(&nnz.to_le_bytes());

        // Pass through extra bytes beyond pos3+normal3 (UV, color, etc.)
        if stride > 24 {
            let mesh_off = i * stride;
            if mesh_off + stride <= mesh_bytes.len() {
                output[out_off + 24..out_off + stride]
                    .copy_from_slice(&mesh_bytes[mesh_off + 24..mesh_off + stride]);
            }
        }
    }

    let mut out = HashMap::new();
    out.insert("deformed_mesh".to_string(), Message::bytes(output));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "vertexCount": vertex_count,
            "boneCount": bone_count,
            "stride": stride,
            "format": if stride > 24 { "pos3_normal3_color3_f32" } else { "pos3_normal3_f32" },
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
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .unwrap_or_default()
}
