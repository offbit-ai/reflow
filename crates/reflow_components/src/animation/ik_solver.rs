//! IK Solver — inverse kinematics for skeletal animation.
//!
//! Two modes:
//! - **Two-bone IK** — solves for arm/leg chains (shoulder→elbow→hand)
//! - **Look-at** — rotates a single bone (head) to face a target
//!
//! Applies IK on top of existing bone transforms from the animation sampler.
//!
//! ## Config
//! ```json
//! {
//!   "chains": [
//!     { "mode": "two_bone", "root": 0, "mid": 1, "end": 2, "weight": 1.0 },
//!     { "mode": "look_at", "bone": 5, "axis": "z", "weight": 0.8 }
//!   ]
//! }
//! ```
//!
//! ## Inports
//! - `bone_transforms` — input pose (Bytes, 64 bytes/bone)
//! - `targets` — IK targets (Object: array of {x,y,z} per chain)
//!
//! ## Outports
//! - `bone_transforms` — pose with IK applied

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

use super::math_helpers::*;

#[actor(
    IKSolverActor,
    inports::<10>(bone_transforms, targets),
    outports::<1>(bone_transforms, metadata),
    state(MemoryState),
    await_inports(bone_transforms)
)]
pub async fn ik_solver_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let mut bones_bytes = match payload.get("bone_transforms") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Ok(HashMap::new()),
    };

    // Cache targets
    if let Some(Message::Object(obj)) = payload.get("targets") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_ik", "targets", v);
    }

    let targets: Value = ctx
        .get_pool("_ik")
        .into_iter()
        .find(|(k, _)| k == "targets")
        .map(|(_, v)| v)
        .unwrap_or(json!([]));

    let chains = config
        .get("chains")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let bone_count = bones_bytes.len() / 64;
    let target_arr = targets.as_array();

    for (ci, chain) in chains.iter().enumerate() {
        let mode = chain
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("two_bone");
        let weight = chain.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;

        let target_pos = target_arr
            .and_then(|a| a.get(ci))
            .map(|t| {
                [
                    t.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    t.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    t.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                ]
            })
            .unwrap_or([0.0; 3]);

        match mode {
            "two_bone" => {
                let root = chain.get("root").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let mid = chain.get("mid").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                let end = chain.get("end").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
                if root < bone_count && mid < bone_count && end < bone_count {
                    solve_two_bone_ik(&mut bones_bytes, root, mid, end, target_pos, weight);
                }
            }
            "look_at" => {
                let bone = chain.get("bone").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                if bone < bone_count {
                    solve_look_at(&mut bones_bytes, bone, target_pos, weight);
                }
            }
            _ => {}
        }
    }

    let mut out = HashMap::new();
    out.insert("bone_transforms".to_string(), Message::bytes(bones_bytes));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "boneCount": bone_count,
            "chainCount": chains.len(),
        }))),
    );
    Ok(out)
}

fn read_mat4(bytes: &[u8], bone: usize) -> [f32; 16] {
    let off = bone * 64;
    let mut m = [0.0f32; 16];
    for j in 0..16 {
        m[j] = f32::from_le_bytes(bytes[off + j * 4..off + j * 4 + 4].try_into().unwrap());
    }
    m
}

fn write_mat4(bytes: &mut [u8], bone: usize, m: &[f32; 16]) {
    let off = bone * 64;
    for j in 0..16 {
        bytes[off + j * 4..off + j * 4 + 4].copy_from_slice(&m[j].to_le_bytes());
    }
}

fn mat4_pos(m: &[f32; 16]) -> [f32; 3] {
    [m[12], m[13], m[14]]
}

fn vec3_sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn vec3_len(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn vec3_norm(v: [f32; 3]) -> [f32; 3] {
    let l = vec3_len(v);
    if l > 1e-6 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn vec3_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn vec3_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn vec3_lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Two-bone IK: adjust root and mid bones so end reaches target
fn solve_two_bone_ik(
    bytes: &mut [u8],
    root: usize,
    mid: usize,
    end: usize,
    target: [f32; 3],
    weight: f32,
) {
    let root_m = read_mat4(bytes, root);
    let mid_m = read_mat4(bytes, mid);
    let end_m = read_mat4(bytes, end);

    let root_pos = mat4_pos(&root_m);
    let mid_pos = mat4_pos(&mid_m);
    let end_pos = mat4_pos(&end_m);

    let upper_len = vec3_len(vec3_sub(mid_pos, root_pos));
    let lower_len = vec3_len(vec3_sub(end_pos, mid_pos));

    let to_target = vec3_sub(target, root_pos);
    let target_dist = vec3_len(to_target).clamp(0.001, upper_len + lower_len - 0.001);

    // Law of cosines for elbow angle
    let cos_angle = ((upper_len * upper_len + lower_len * lower_len - target_dist * target_dist)
        / (2.0 * upper_len * lower_len))
        .clamp(-1.0, 1.0);
    let _elbow_angle = cos_angle.acos();

    // Simplified IK: move mid bone to maintain chain length toward target
    let dir = vec3_norm(to_target);
    let new_mid = [
        root_pos[0] + dir[0] * upper_len,
        root_pos[1] + dir[1] * upper_len,
        root_pos[2] + dir[2] * upper_len,
    ];
    let new_end = target;

    // Apply with weight
    let final_mid = vec3_lerp(mid_pos, new_mid, weight);
    let final_end = vec3_lerp(end_pos, new_end, weight);

    // Update translation components of bone matrices
    let mut new_mid_m = mid_m;
    new_mid_m[12] = final_mid[0];
    new_mid_m[13] = final_mid[1];
    new_mid_m[14] = final_mid[2];
    write_mat4(bytes, mid, &new_mid_m);

    let mut new_end_m = end_m;
    new_end_m[12] = final_end[0];
    new_end_m[13] = final_end[1];
    new_end_m[14] = final_end[2];
    write_mat4(bytes, end, &new_end_m);
}

/// Look-at IK: rotate bone to face target
fn solve_look_at(bytes: &mut [u8], bone: usize, target: [f32; 3], weight: f32) {
    let m = read_mat4(bytes, bone);
    let pos = mat4_pos(&m);
    let dir = vec3_norm(vec3_sub(target, pos));

    // Build look-at rotation matrix
    let forward = dir;
    let right = vec3_norm(vec3_cross([0.0, 1.0, 0.0], forward));
    let up = vec3_cross(forward, right);

    let mut look_m = MAT4_IDENTITY;
    look_m[0] = right[0];
    look_m[1] = right[1];
    look_m[2] = right[2];
    look_m[4] = up[0];
    look_m[5] = up[1];
    look_m[6] = up[2];
    look_m[8] = forward[0];
    look_m[9] = forward[1];
    look_m[10] = forward[2];
    look_m[12] = pos[0];
    look_m[13] = pos[1];
    look_m[14] = pos[2];

    // Blend with original
    let mut blended = [0.0f32; 16];
    for j in 0..16 {
        blended[j] = m[j] * (1.0 - weight) + look_m[j] * weight;
    }
    write_mat4(bytes, bone, &blended);
}
