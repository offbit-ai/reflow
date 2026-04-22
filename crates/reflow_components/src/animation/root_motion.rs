//! Root Motion — extracts hip/root bone translation as character movement delta.
//!
//! Strips root bone translation from bone transforms and outputs it separately
//! as a position delta, so the character controller can apply world-space movement.
//!
//! ## Config
//! - `rootBone` — index of root bone (default 0, typically Hips)
//! - `extractX/Y/Z` — which axes to extract (default: X and Z for ground movement)
//!
//! ## Inports
//! - `bone_transforms` — input pose (per frame)
//!
//! ## Outports
//! - `bone_transforms` — pose with root translation removed on extracted axes
//! - `delta` — Object { x, y, z } position delta this frame
//! - `velocity` — Object { x, y, z } velocity (delta / dt)

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    RootMotionActor,
    inports::<10>(bone_transforms, dt),
    outports::<1>(bone_transforms, delta, velocity),
    state(MemoryState),
    await_inports(bone_transforms)
)]
pub async fn root_motion_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let root_bone = config.get("rootBone").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let extract_x = config
        .get("extractX")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let extract_y = config
        .get("extractY")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let extract_z = config
        .get("extractZ")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let dt = match payload.get("dt") {
        Some(Message::Float(f)) => *f as f32,
        _ => 1.0 / 30.0,
    };

    let mut bytes = match payload.get("bone_transforms") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Ok(HashMap::new()),
    };

    let bone_count = bytes.len() / 64;
    if root_bone >= bone_count {
        let mut out = HashMap::new();
        out.insert("bone_transforms".to_string(), Message::bytes(bytes));
        return Ok(out);
    }

    // Read previous root position from pool
    let prev_pool: HashMap<String, Value> = ctx.get_pool("_root").into_iter().collect();
    let prev_x = prev_pool.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let prev_y = prev_pool.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let prev_z = prev_pool.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

    // Read current root position
    let off = root_bone * 64;
    let rx = f32::from_le_bytes(bytes[off + 48..off + 52].try_into().unwrap()); // m[12]
    let ry = f32::from_le_bytes(bytes[off + 52..off + 56].try_into().unwrap()); // m[13]
    let rz = f32::from_le_bytes(bytes[off + 56..off + 60].try_into().unwrap()); // m[14]

    // Compute delta
    let dx = if extract_x { rx - prev_x } else { 0.0 };
    let dy = if extract_y { ry - prev_y } else { 0.0 };
    let dz = if extract_z { rz - prev_z } else { 0.0 };

    // Store current position
    ctx.pool_upsert("_root", "x", json!(rx));
    ctx.pool_upsert("_root", "y", json!(ry));
    ctx.pool_upsert("_root", "z", json!(rz));

    // Strip extracted axes from root bone translation
    if extract_x {
        bytes[off + 48..off + 52].copy_from_slice(&0.0f32.to_le_bytes());
    }
    if extract_y {
        bytes[off + 52..off + 56].copy_from_slice(&0.0f32.to_le_bytes());
    }
    if extract_z {
        bytes[off + 56..off + 60].copy_from_slice(&0.0f32.to_le_bytes());
    }

    let mut out = HashMap::new();
    out.insert("bone_transforms".to_string(), Message::bytes(bytes));
    out.insert(
        "delta".to_string(),
        Message::object(EncodableValue::from(json!({ "x": dx, "y": dy, "z": dz }))),
    );
    let vx = if dt > 0.0 { dx / dt } else { 0.0 };
    let vy = if dt > 0.0 { dy / dt } else { 0.0 };
    let vz = if dt > 0.0 { dz / dt } else { 0.0 };
    out.insert(
        "velocity".to_string(),
        Message::object(EncodableValue::from(json!({ "x": vx, "y": vy, "z": vz }))),
    );
    Ok(out)
}
