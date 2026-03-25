//! Character Controller — translates input into locomotion state + movement.
//!
//! Receives input events (keyboard/gamepad) and outputs:
//! - Movement direction and speed
//! - Animation state name for the Animation FSM
//! - World-space position/rotation for the scene graph
//!
//! ## Config
//! ```json
//! {
//!   "walkSpeed": 150.0,
//!   "runSpeed": 350.0,
//!   "turnSpeed": 180.0,
//!   "gravity": -980.0,
//!   "jumpForce": 400.0,
//!   "states": {
//!     "idle": { "maxSpeed": 0 },
//!     "walk": { "maxSpeed": 150 },
//!     "run":  { "maxSpeed": 350 },
//!     "jump": { "maxSpeed": 150 }
//!   }
//! }
//! ```
//!
//! ## Inports
//! - `input` — Object { moveX, moveY, run, jump } from input mapper
//! - `dt` — delta time per frame
//! - `root_delta` — root motion delta from RootMotionActor (optional)
//!
//! ## Outports
//! - `state` — String (animation state name: "idle", "walk", "run", "jump")
//! - `position` — Object { x, y, z } world position
//! - `rotation` — Object { x, y, z } Euler rotation (degrees)
//! - `velocity` — Object { x, y, z } current velocity

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    CharacterControllerActor,
    inports::<10>(input, dt, root_delta),
    outports::<1>(state, position, rotation, velocity, metadata),
    state(MemoryState),
    await_inports(dt)
)]
pub async fn character_controller_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let walk_speed = config
        .get("walkSpeed")
        .and_then(|v| v.as_f64())
        .unwrap_or(150.0) as f32;
    let run_speed = config
        .get("runSpeed")
        .and_then(|v| v.as_f64())
        .unwrap_or(350.0) as f32;
    let turn_speed = config
        .get("turnSpeed")
        .and_then(|v| v.as_f64())
        .unwrap_or(180.0) as f32;
    let gravity = config
        .get("gravity")
        .and_then(|v| v.as_f64())
        .unwrap_or(-980.0) as f32;
    let jump_force = config
        .get("jumpForce")
        .and_then(|v| v.as_f64())
        .unwrap_or(400.0) as f32;

    let dt = match payload.get("dt") {
        Some(Message::Float(f)) => *f as f32,
        Some(Message::Integer(i)) => *i as f32,
        _ => 1.0 / 30.0,
    };

    // Cache input
    if let Some(Message::Object(obj)) = payload.get("input") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_ctrl", "input", v);
    }

    // Read cached input
    let pool: HashMap<String, Value> = ctx.get_pool("_ctrl").into_iter().collect();
    let input = pool.get("input").cloned().unwrap_or(json!({}));

    let move_x = input.get("moveX").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let move_y = input.get("moveY").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let is_run = input.get("run").and_then(|v| v.as_bool()).unwrap_or(false);
    let is_jump = input.get("jump").and_then(|v| v.as_bool()).unwrap_or(false);

    // Read state from pool
    let mut pos_x = pool.get("px").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let mut pos_y = pool.get("py").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let mut pos_z = pool.get("pz").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let mut rot_y = pool.get("ry").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let mut vel_y = pool.get("vy").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let grounded = pos_y <= 0.01;

    // Apply root motion delta if available
    if let Some(Message::Object(obj)) = payload.get("root_delta") {
        let v: Value = obj.as_ref().clone().into();
        pos_x += v.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        pos_y += v.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        pos_z += v.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    }

    // Determine move speed and state
    let input_mag = (move_x * move_x + move_y * move_y).sqrt().min(1.0);
    let speed = if is_run { run_speed } else { walk_speed };

    let anim_state = if !grounded {
        "jump"
    } else if input_mag > 0.1 {
        if is_run {
            "run"
        } else {
            "walk"
        }
    } else {
        "idle"
    };

    // Turn toward input direction
    if input_mag > 0.1 {
        let target_angle = move_x.atan2(move_y).to_degrees();
        let diff = target_angle - rot_y;
        let diff = ((diff + 540.0) % 360.0) - 180.0; // shortest path
        rot_y += diff.clamp(-turn_speed * dt, turn_speed * dt);
    }

    // Movement
    let rad = rot_y.to_radians();
    let fwd_x = rad.sin();
    let fwd_z = rad.cos();
    pos_x += fwd_x * speed * input_mag * dt;
    pos_z += fwd_z * speed * input_mag * dt;

    // Vertical: gravity + jump
    if grounded && is_jump {
        vel_y = jump_force;
    }
    vel_y += gravity * dt;
    pos_y += vel_y * dt;
    if pos_y < 0.0 {
        pos_y = 0.0;
        vel_y = 0.0;
    }

    // Persist state
    ctx.pool_upsert("_ctrl", "px", json!(pos_x));
    ctx.pool_upsert("_ctrl", "py", json!(pos_y));
    ctx.pool_upsert("_ctrl", "pz", json!(pos_z));
    ctx.pool_upsert("_ctrl", "ry", json!(rot_y));
    ctx.pool_upsert("_ctrl", "vy", json!(vel_y));

    let mut out = HashMap::new();
    out.insert(
        "state".to_string(),
        Message::String(std::sync::Arc::new(anim_state.to_string())),
    );
    out.insert(
        "position".to_string(),
        Message::object(EncodableValue::from(
            json!({ "x": pos_x, "y": pos_y, "z": pos_z }),
        )),
    );
    out.insert(
        "rotation".to_string(),
        Message::object(EncodableValue::from(
            json!({ "x": 0.0, "y": rot_y, "z": 0.0 }),
        )),
    );
    out.insert(
        "velocity".to_string(),
        Message::object(EncodableValue::from(
            json!({ "x": fwd_x * speed * input_mag, "y": vel_y, "z": fwd_z * speed * input_mag }),
        )),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "state": anim_state,
            "speed": speed * input_mag,
            "grounded": grounded,
        }))),
    );
    Ok(out)
}
