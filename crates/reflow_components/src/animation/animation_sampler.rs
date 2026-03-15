//! Animation sampler — samples a clip at time t, outputs bone pose matrices.
//!
//! Walks the bone hierarchy, interpolates keyframes, computes world transforms,
//! and multiplies by inverse bind matrices to produce final skinning matrices.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::{json, Value};
use std::collections::HashMap;

use super::math_helpers::*;

#[actor(
    AnimationSamplerActor,
    inports::<10>(clip, time, skeleton, inverse_bind_matrices),
    outports::<1>(bone_transforms, metadata),
    state(MemoryState),
    await_all_inports
)]
pub async fn animation_sampler_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let do_loop = config
        .get("loop")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Parse time
    let time = match payload.get("time") {
        Some(Message::Float(f)) => *f as f32,
        Some(Message::Integer(i)) => *i as f32,
        _ => 0.0,
    };

    // Parse clip
    let clip: Value = match payload.get("clip") {
        Some(Message::Object(obj)) => obj.as_ref().clone().into(),
        _ => return Err(anyhow::anyhow!("Expected clip on clip port")),
    };

    let duration = clip.get("duration").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let channels = clip
        .get("channels")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Parse skeleton
    let skeleton: Value = match payload.get("skeleton") {
        Some(Message::Object(obj)) => obj.as_ref().clone().into(),
        _ => return Err(anyhow::anyhow!("Expected skeleton")),
    };

    let bones = skeleton
        .get("bones")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("Skeleton missing bones"))?;
    let bone_count = bones.len();

    // Parse inverse bind matrices (packed f32 bytes, bone_count * 64)
    let ibm_bytes = match payload.get("inverse_bind_matrices") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => vec![],
    };

    // Wrap time
    let t = if do_loop && duration > 0.0 {
        time % duration
    } else {
        time.min(duration)
    };

    // Initialize per-bone local transforms to bind pose
    let mut local_transforms: Vec<([f32; 3], [f32; 4], [f32; 3])> = bones
        .iter()
        .map(|b| {
            let local = b
                .get("localBindTransform")
                .and_then(|v| v.as_array())
                .map(|a| {
                    let mut m = [0.0f32; 16];
                    for (i, v) in a.iter().enumerate().take(16) {
                        m[i] = v.as_f64().unwrap_or(0.0) as f32;
                    }
                    m
                })
                .unwrap_or(MAT4_IDENTITY);
            // Extract TRS from bind transform (approximate: just use translation)
            let pos = [local[12], local[13], local[14]];
            let rot = [0.0f32, 0.0, 0.0, 1.0]; // identity rotation as default
            let scl = [1.0f32; 3];
            (pos, rot, scl)
        })
        .collect();

    // Sample each channel at time t
    for ch in &channels {
        let bone_idx = ch.get("boneIndex").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        if bone_idx >= bone_count {
            continue;
        }
        let property = ch
            .get("property")
            .and_then(|v| v.as_str())
            .unwrap_or("rotation");
        let interp = ch
            .get("interpolation")
            .and_then(|v| v.as_str())
            .unwrap_or("linear");
        let times = ch.get("times").and_then(|v| v.as_array());
        let values = ch.get("values").and_then(|v| v.as_array());

        let (times, values) = match (times, values) {
            (Some(t), Some(v)) => (t, v),
            _ => continue,
        };

        if times.is_empty() {
            continue;
        }

        // Binary search for bracketing keyframes
        let (idx0, idx1, frac) = find_keyframe_pair(times, t);

        match property {
            "position" => {
                let v0 = parse_vec3_value(values.get(idx0));
                let v1 = parse_vec3_value(values.get(idx1));
                local_transforms[bone_idx].0 = match interp {
                    "step" => v0,
                    _ => vec3_lerp(v0, v1, frac),
                };
            }
            "rotation" => {
                let v0 = parse_quat_value(values.get(idx0));
                let v1 = parse_quat_value(values.get(idx1));
                local_transforms[bone_idx].1 = match interp {
                    "step" => v0,
                    _ => quat_slerp(v0, v1, frac),
                };
            }
            "scale" => {
                let v0 = parse_vec3_value(values.get(idx0));
                let v1 = parse_vec3_value(values.get(idx1));
                local_transforms[bone_idx].2 = match interp {
                    "step" => v0,
                    _ => vec3_lerp(v0, v1, frac),
                };
            }
            _ => {}
        }
    }

    // Build local matrices from sampled TRS
    let local_matrices: Vec<[f32; 16]> = local_transforms
        .iter()
        .map(|(p, r, s)| trs_to_mat4(*p, *r, *s))
        .collect();

    // Walk hierarchy: world = parent_world * local
    let parents: Vec<i32> = bones
        .iter()
        .map(|b| b.get("parent").and_then(|v| v.as_i64()).unwrap_or(-1) as i32)
        .collect();

    let mut world_transforms: Vec<[f32; 16]> = vec![MAT4_IDENTITY; bone_count];
    for i in 0..bone_count {
        let p = parents[i];
        if p >= 0 && (p as usize) < bone_count {
            world_transforms[i] = mat4_mul(&world_transforms[p as usize], &local_matrices[i]);
        } else {
            world_transforms[i] = local_matrices[i];
        }
    }

    // Multiply by inverse bind matrices: skinMatrix = world * ibm
    let mut skin_matrices: Vec<[f32; 16]> = Vec::with_capacity(bone_count);
    for i in 0..bone_count {
        let ibm = if ibm_bytes.len() >= (i + 1) * 64 {
            let off = i * 64;
            let mut m = [0.0f32; 16];
            for j in 0..16 {
                m[j] = f32::from_le_bytes(
                    ibm_bytes[off + j * 4..off + j * 4 + 4].try_into().unwrap(),
                );
            }
            m
        } else {
            MAT4_IDENTITY
        };
        skin_matrices.push(mat4_mul(&world_transforms[i], &ibm));
    }

    // Pack as bytes: bone_count * 16 floats * 4 bytes = bone_count * 64
    let mut out_bytes = Vec::with_capacity(bone_count * 64);
    for m in &skin_matrices {
        for f in m {
            out_bytes.extend_from_slice(&f.to_le_bytes());
        }
    }

    let mut out = HashMap::new();
    out.insert("bone_transforms".to_string(), Message::bytes(out_bytes));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "boneCount": bone_count,
            "time": t,
            "duration": duration,
        }))),
    );
    Ok(out)
}

fn find_keyframe_pair(times: &[Value], t: f32) -> (usize, usize, f32) {
    let n = times.len();
    if n == 0 {
        return (0, 0, 0.0);
    }
    if n == 1 {
        return (0, 0, 0.0);
    }

    let last = times[n - 1].as_f64().unwrap_or(1.0) as f32;
    if t >= last {
        return (n - 1, n - 1, 0.0);
    }

    let first = times[0].as_f64().unwrap_or(0.0) as f32;
    if t <= first {
        return (0, 0, 0.0);
    }

    // Binary search
    let mut lo = 0;
    let mut hi = n - 1;
    while lo < hi - 1 {
        let mid = (lo + hi) / 2;
        let mid_t = times[mid].as_f64().unwrap_or(0.0) as f32;
        if t < mid_t {
            hi = mid;
        } else {
            lo = mid;
        }
    }

    let t0 = times[lo].as_f64().unwrap_or(0.0) as f32;
    let t1 = times[hi].as_f64().unwrap_or(1.0) as f32;
    let frac = if (t1 - t0).abs() > 1e-8 {
        (t - t0) / (t1 - t0)
    } else {
        0.0
    };

    (lo, hi, frac.clamp(0.0, 1.0))
}

fn parse_vec3_value(v: Option<&Value>) -> [f32; 3] {
    match v {
        Some(Value::Array(a)) if a.len() >= 3 => [
            a[0].as_f64().unwrap_or(0.0) as f32,
            a[1].as_f64().unwrap_or(0.0) as f32,
            a[2].as_f64().unwrap_or(0.0) as f32,
        ],
        _ => [0.0; 3],
    }
}

fn parse_quat_value(v: Option<&Value>) -> [f32; 4] {
    match v {
        Some(Value::Array(a)) if a.len() >= 4 => [
            a[0].as_f64().unwrap_or(0.0) as f32,
            a[1].as_f64().unwrap_or(0.0) as f32,
            a[2].as_f64().unwrap_or(0.0) as f32,
            a[3].as_f64().unwrap_or(1.0) as f32,
        ],
        _ => [0.0, 0.0, 0.0, 1.0],
    }
}
