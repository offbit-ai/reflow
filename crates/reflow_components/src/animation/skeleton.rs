//! Skeleton actor — defines a bone hierarchy with inverse bind matrices.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::{json, Value};
use std::collections::HashMap;

use super::math_helpers::{mat4_inverse, mat4_mul, trs_to_mat4, MAT4_IDENTITY};

#[actor(
    SkeletonActor,
    inports::<10>(bones),
    outports::<1>(skeleton, metadata),
    state(MemoryState)
)]
pub async fn skeleton_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let name = config
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("skeleton")
        .to_string();

    // High-level config: generate bones from boneCount + spacing + axis
    // OR explicit bones array (backward compatible)
    let bones_json = if let Some(count_val) = config.get("boneCount") {
        let bone_count = count_val.as_u64().unwrap_or(1) as usize;
        let spacing = config
            .get("spacing")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);
        let axis = config.get("axis").and_then(|v| v.as_str()).unwrap_or("x");
        let start = parse_vec3(config.get("startPosition"), [0.0; 3]);

        let dir: [f64; 3] = match axis {
            "y" | "Y" => [0.0, 1.0, 0.0],
            "z" | "Z" => [0.0, 0.0, 1.0],
            _ => [1.0, 0.0, 0.0], // default: x
        };

        let mut bones = Vec::with_capacity(bone_count);
        for i in 0..bone_count {
            // Root bone gets world startPosition; children get local offset along axis
            let pos = if i == 0 {
                [start[0] as f64, start[1] as f64, start[2] as f64]
            } else {
                [dir[0] * spacing, dir[1] * spacing, dir[2] * spacing]
            };
            bones.push(json!({
                "name": format!("bone_{}", i),
                "parent": if i == 0 { -1i64 } else { i as i64 - 1 },
                "bindPosition": pos,
                "bindRotation": [0, 0, 0, 1],
                "bindScale": [1, 1, 1],
            }));
        }
        json!(bones)
    } else {
        config.get("bones").cloned().unwrap_or_else(|| json!([]))
    };

    let bones = bones_json
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("bones must be an array"))?;

    let bone_count = bones.len();

    // Build local bind transforms and hierarchy
    let mut local_bind: Vec<[f32; 16]> = Vec::with_capacity(bone_count);
    let mut parents: Vec<i32> = Vec::with_capacity(bone_count);
    let mut bone_names: Vec<String> = Vec::with_capacity(bone_count);

    for bone in bones {
        let bname = bone
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("bone")
            .to_string();
        let parent = bone.get("parent").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;

        let pos = parse_vec3(bone.get("bindPosition"), [0.0; 3]);
        let rot = parse_quat(bone.get("bindRotation"), [0.0, 0.0, 0.0, 1.0]);
        let scl = parse_vec3(bone.get("bindScale"), [1.0; 3]);

        local_bind.push(trs_to_mat4(pos, rot, scl));
        parents.push(parent);
        bone_names.push(bname);
    }

    // Compute world bind matrices (parent * local, in topological order)
    let mut world_bind: Vec<[f32; 16]> = vec![MAT4_IDENTITY; bone_count];
    for i in 0..bone_count {
        let parent_idx = parents[i];
        if parent_idx >= 0 && (parent_idx as usize) < bone_count {
            world_bind[i] = mat4_mul(&world_bind[parent_idx as usize], &local_bind[i]);
        } else {
            world_bind[i] = local_bind[i];
        }
    }

    // Compute inverse bind matrices
    let mut inverse_bind: Vec<[f32; 16]> = Vec::with_capacity(bone_count);
    for i in 0..bone_count {
        inverse_bind.push(mat4_inverse(&world_bind[i]));
    }

    // Pack inverse bind matrices as flat f32 bytes (bone_count * 64 bytes)
    let mut ibm_bytes = Vec::with_capacity(bone_count * 64);
    for m in &inverse_bind {
        for f in m {
            ibm_bytes.extend_from_slice(&f.to_le_bytes());
        }
    }

    // Build output JSON
    let bones_out: Vec<Value> = (0..bone_count)
        .map(|i| {
            json!({
                "index": i,
                "name": bone_names[i],
                "parent": parents[i],
                "localBindTransform": local_bind[i].to_vec(),
            })
        })
        .collect();

    let skeleton = json!({
        "name": name,
        "boneCount": bone_count,
        "bones": bones_out,
    });

    let mut out = HashMap::new();
    out.insert(
        "skeleton".to_string(),
        Message::object(EncodableValue::from(skeleton)),
    );
    // Also output IBM bytes for efficient downstream use
    out.insert(
        "inverse_bind_matrices".to_string(),
        Message::bytes(ibm_bytes),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "name": name,
            "boneCount": bone_count,
        }))),
    );
    Ok(out)
}

fn parse_vec3(v: Option<&Value>, default: [f32; 3]) -> [f32; 3] {
    match v {
        Some(Value::Array(a)) if a.len() >= 3 => [
            a[0].as_f64().unwrap_or(default[0] as f64) as f32,
            a[1].as_f64().unwrap_or(default[1] as f64) as f32,
            a[2].as_f64().unwrap_or(default[2] as f64) as f32,
        ],
        _ => default,
    }
}

fn parse_quat(v: Option<&Value>, default: [f32; 4]) -> [f32; 4] {
    match v {
        Some(Value::Array(a)) if a.len() >= 4 => [
            a[0].as_f64().unwrap_or(default[0] as f64) as f32,
            a[1].as_f64().unwrap_or(default[1] as f64) as f32,
            a[2].as_f64().unwrap_or(default[2] as f64) as f32,
            a[3].as_f64().unwrap_or(default[3] as f64) as f32,
        ],
        _ => default,
    }
}
