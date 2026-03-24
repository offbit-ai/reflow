//! Animation FSM — state machine for animation transitions with cross-fade blending.
//!
//! Each state maps to an animation clip index (input port). Transitions between
//! states cross-fade over a configurable duration. The FSM receives events on the
//! `event` port and ticks on `bone_transforms` to drive the blend.
//!
//! ## Config
//! ```json
//! {
//!   "initial": "idle",
//!   "states": {
//!     "idle": { "input": 0 },
//!     "walk": { "input": 1 },
//!     "run":  { "input": 2 }
//!   },
//!   "transitions": {
//!     "idle->walk": { "duration": 0.3 },
//!     "walk->run":  { "duration": 0.2 },
//!     "*->idle":    { "duration": 0.5 }
//!   }
//! }
//! ```
//!
//! ## Inports
//! - `input_0..input_7` — bone transforms per animation state
//! - `event` — state change event (String: target state name)
//! - `dt` — delta time per frame (Float)
//!
//! ## Outports
//! - `bone_transforms` — cross-faded output
//! - `state` — current state name (String)

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    AnimationFsmActor,
    inports::<10>(input_0, input_1, input_2, input_3, input_4, input_5, input_6, input_7, event, dt),
    outports::<1>(bone_transforms, state, metadata),
    state(MemoryState),
    await_inports(dt)
)]
pub async fn animation_fsm_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let states_cfg = config.get("states").cloned().unwrap_or(json!({}));
    let transitions_cfg = config.get("transitions").cloned().unwrap_or(json!({}));
    let initial = config.get("initial").and_then(|v| v.as_str()).unwrap_or("idle");

    // Cache inputs
    for i in 0..8 {
        let port = format!("input_{}", i);
        if let Some(Message::Bytes(b)) = payload.get(&port) {
            use base64::Engine;
            ctx.pool_upsert(
                "_inputs",
                &port,
                json!(base64::engine::general_purpose::STANDARD.encode(&**b)),
            );
        }
    }

    // Read FSM state from pool
    let fsm_pool: HashMap<String, Value> = ctx.get_pool("_fsm").into_iter().collect();
    let mut current_state = fsm_pool
        .get("current")
        .and_then(|v| v.as_str())
        .unwrap_or(initial)
        .to_string();
    let mut prev_state = fsm_pool
        .get("prev")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut blend_time = fsm_pool
        .get("blend_time")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    let mut blend_duration = fsm_pool
        .get("blend_duration")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;

    let dt = match payload.get("dt") {
        Some(Message::Float(f)) => *f as f32,
        Some(Message::Integer(i)) => *i as f32,
        _ => 1.0 / 30.0,
    };

    // Handle state change event
    if let Some(msg) = payload.get("event") {
        let target = match msg {
            Message::String(s) => s.to_string(),
            Message::Object(obj) => {
                let v: Value = obj.as_ref().clone().into();
                v.get("target").and_then(|v| v.as_str()).unwrap_or("").to_string()
            }
            _ => String::new(),
        };
        if !target.is_empty() && target != current_state {
            // Look up transition duration
            let key1 = format!("{}->{}",current_state, target);
            let key2 = format!("*->{}", target);
            let dur = transitions_cfg.get(&key1)
                .or_else(|| transitions_cfg.get(&key2))
                .and_then(|v| v.get("duration"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.3) as f32;

            prev_state = current_state.clone();
            current_state = target;
            blend_time = 0.0;
            blend_duration = dur;
        }
    }

    // Advance blend timer
    blend_time += dt;

    // Persist FSM state
    ctx.pool_upsert("_fsm", "current", json!(current_state));
    ctx.pool_upsert("_fsm", "prev", json!(prev_state));
    ctx.pool_upsert("_fsm", "blend_time", json!(blend_time));
    ctx.pool_upsert("_fsm", "blend_duration", json!(blend_duration));

    // Resolve input index for a state name
    let resolve_input = |state: &str| -> usize {
        states_cfg
            .get(state)
            .and_then(|v| v.get("input"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize
    };

    // Get cached input bytes
    let get_input = |idx: usize| -> Option<Vec<u8>> {
        let key = format!("input_{}", idx);
        let cache: HashMap<String, Value> = ctx.get_pool("_inputs").into_iter().collect();
        cache.get(&key).and_then(|v| v.as_str()).and_then(|s| {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.decode(s).ok()
        })
    };

    let current_idx = resolve_input(&current_state);
    let current_bytes = match get_input(current_idx) {
        Some(b) => b,
        None => return Ok(HashMap::new()),
    };

    let bone_count = current_bytes.len() / 64;

    // Cross-fade if transitioning
    let output = if !prev_state.is_empty() && blend_duration > 0.0 && blend_time < blend_duration {
        let alpha = (blend_time / blend_duration).clamp(0.0, 1.0);
        let prev_idx = resolve_input(&prev_state);
        if let Some(prev_bytes) = get_input(prev_idx) {
            // Lerp from prev to current
            let mut out = Vec::with_capacity(bone_count * 64);
            for b in 0..bone_count {
                let off = b * 64;
                for j in 0..16 {
                    let va = f32::from_le_bytes(prev_bytes[off + j * 4..off + j * 4 + 4].try_into().unwrap());
                    let vb = f32::from_le_bytes(current_bytes[off + j * 4..off + j * 4 + 4].try_into().unwrap());
                    let r = va * (1.0 - alpha) + vb * alpha;
                    out.extend_from_slice(&r.to_le_bytes());
                }
            }
            out
        } else {
            current_bytes
        }
    } else {
        current_bytes
    };

    let mut out = HashMap::new();
    out.insert("bone_transforms".to_string(), Message::bytes(output));
    out.insert(
        "state".to_string(),
        Message::String(std::sync::Arc::new(current_state.clone())),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "state": current_state,
            "blending": blend_time < blend_duration,
            "blendProgress": if blend_duration > 0.0 { blend_time / blend_duration } else { 1.0 },
        }))),
    );
    Ok(out)
}
