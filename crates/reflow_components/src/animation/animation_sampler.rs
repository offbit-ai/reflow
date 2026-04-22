//! Animation sampler — samples a clip at time t, outputs bone pose matrices.
//!
//! Caches clip, skeleton, and IBM data in state on first receipt. Fires
//! on every `time` input once all required data has been cached.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

use super::math_helpers::*;

#[actor(
    AnimationSamplerActor,
    inports::<10>(clip, time, skeleton, inverse_bind_matrices),
    outports::<1>(bone_transforms, metadata),
    state(MemoryState),
    await_inports(time)
)]
pub async fn animation_sampler_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let do_loop = config.get("loop").and_then(|v| v.as_bool()).unwrap_or(true);

    // Cache static inputs in pool on first receipt
    if let Some(Message::Object(obj)) = payload.get("clip") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_cache", "clip", v);
    }
    if let Some(Message::Object(obj)) = payload.get("skeleton") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_cache", "skeleton", v);
    }
    if let Some(Message::Bytes(b)) = payload.get("inverse_bind_matrices") {
        // Store IBM as base64 in pool (pool only holds JSON values)
        let encoded = base64_encode(&b);
        ctx.pool_upsert("_cache", "ibm_b64", json!(encoded));
    }

    // Get time — guaranteed present via await_inports
    let time = match payload.get("time") {
        Some(Message::Float(f)) => *f as f32,
        Some(Message::Integer(i)) => *i as f32,
        _ => unreachable!("await_inports guarantees time"),
    };

    // Retrieve cached data (clip + skeleton arrive once, cached in pool)
    let cache: HashMap<String, Value> = ctx.get_pool("_cache").into_iter().collect();

    let clip = match cache.get("clip") {
        Some(v) => v.clone(),
        None => return Ok(HashMap::new()), // Not arrived yet
    };
    let skeleton = match cache.get("skeleton") {
        Some(v) => v.clone(),
        None => return Ok(HashMap::new()), // Not arrived yet
    };
    let ibm_bytes: Vec<u8> = cache
        .get("ibm_b64")
        .and_then(|v| v.as_str())
        .map(base64_decode)
        .unwrap_or_default();

    // Parse clip
    let duration = clip.get("duration").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let channels = clip
        .get("channels")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Parse skeleton bones
    let bones = skeleton
        .get("bones")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("Skeleton missing bones"))?;
    let bone_count = bones.len();

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
            let pos = [local[12], local[13], local[14]];
            let rot = [0.0f32, 0.0, 0.0, 1.0];
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

        let (idx0, idx1, frac) = find_keyframe_pair(times, t);

        match property {
            "position" => {
                let v0 = parse_vec3_value(values.get(idx0));
                let v1 = parse_vec3_value(values.get(idx1));
                local_transforms[bone_idx].0 = if interp == "step" {
                    v0
                } else {
                    vec3_lerp(v0, v1, frac)
                };
            }
            "rotation" => {
                let v0 = parse_quat_value(values.get(idx0));
                let v1 = parse_quat_value(values.get(idx1));
                local_transforms[bone_idx].1 = if interp == "step" {
                    v0
                } else {
                    quat_slerp(v0, v1, frac)
                };
            }
            "scale" => {
                let v0 = parse_vec3_value(values.get(idx0));
                let v1 = parse_vec3_value(values.get(idx1));
                local_transforms[bone_idx].2 = if interp == "step" {
                    v0
                } else {
                    vec3_lerp(v0, v1, frac)
                };
            }
            _ => {}
        }
    }

    // Build local matrices
    let local_matrices: Vec<[f32; 16]> = local_transforms
        .iter()
        .map(|(p, r, s)| trs_to_mat4(*p, *r, *s))
        .collect();

    // Walk hierarchy
    let parents: Vec<i32> = bones
        .iter()
        .map(|b| b.get("parent").and_then(|v| v.as_i64()).unwrap_or(-1) as i32)
        .collect();

    let mut world_transforms = vec![MAT4_IDENTITY; bone_count];
    for i in 0..bone_count {
        let p = parents[i];
        world_transforms[i] = if p >= 0 && (p as usize) < bone_count {
            mat4_mul(&world_transforms[p as usize], &local_matrices[i])
        } else {
            local_matrices[i]
        };
    }

    // Multiply by inverse bind matrices
    let mut out_bytes = Vec::with_capacity(bone_count * 64);
    for i in 0..bone_count {
        let ibm = if ibm_bytes.len() >= (i + 1) * 64 {
            let off = i * 64;
            let mut m = [0.0f32; 16];
            for j in 0..16 {
                m[j] =
                    f32::from_le_bytes(ibm_bytes[off + j * 4..off + j * 4 + 4].try_into().unwrap());
            }
            m
        } else {
            MAT4_IDENTITY
        };
        let skin_mat = mat4_mul(&world_transforms[i], &ibm);
        for f in &skin_mat {
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
    if n <= 1 {
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

    let mut lo = 0;
    let mut hi = n - 1;
    while lo < hi - 1 {
        let mid = (lo + hi) / 2;
        if t < times[mid].as_f64().unwrap_or(0.0) as f32 {
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

// Simple base64 encode/decode for storing bytes in JSON pool
fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn base64_decode(s: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .unwrap_or_default()
}
