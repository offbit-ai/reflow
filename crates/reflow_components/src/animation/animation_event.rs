//! Animation Event — emits signals at specific keyframe timestamps.
//!
//! Monitors animation time and fires events when crossing configured timestamps.
//! Used for footstep sounds, particle spawns, damage windows, etc.
//!
//! ## Config
//! ```json
//! {
//!   "events": [
//!     { "time": 0.5, "name": "footstep_left" },
//!     { "time": 1.0, "name": "footstep_right" },
//!     { "time": 1.5, "name": "attack_start", "data": { "damage": 10 } }
//!   ],
//!   "duration": 2.0,
//!   "loop": true
//! }
//! ```
//!
//! ## Inports
//! - `time` — current animation time (Float)
//!
//! ## Outports
//! - `event` — Object { name, time, data } when event fires
//! - `events` — Array of all events that fired this frame

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    AnimationEventActor,
    inports::<10>(time),
    outports::<1>(event, events, metadata),
    state(MemoryState),
    await_inports(time)
)]
pub async fn animation_event_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let event_defs = config
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let duration = config.get("duration").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let do_loop = config.get("loop").and_then(|v| v.as_bool()).unwrap_or(true);

    let current_time = match payload.get("time") {
        Some(Message::Float(f)) => *f,
        Some(Message::Integer(i)) => *i as f64,
        _ => 0.0,
    };

    // Wrap time if looping
    let t = if do_loop && duration > 0.0 {
        current_time % duration
    } else {
        current_time
    };

    // Read previous time from pool
    let pool: HashMap<String, Value> = ctx.get_pool("_evt").into_iter().collect();
    let prev_t = pool.get("prev_time").and_then(|v| v.as_f64()).unwrap_or(-1.0);
    ctx.pool_upsert("_evt", "prev_time", json!(t));

    // Find events that fire between prev_t and t
    let mut fired: Vec<Value> = Vec::new();
    for evt in &event_defs {
        let evt_time = evt.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let name = evt.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let data = evt.get("data").cloned().unwrap_or(json!(null));

        // Check if event time is between prev_t and t (handles loop wrap)
        let fires = if t >= prev_t {
            evt_time > prev_t && evt_time <= t
        } else {
            // Loop wrapped: check both sides
            evt_time > prev_t || evt_time <= t
        };

        if fires && prev_t >= 0.0 {
            fired.push(json!({
                "name": name,
                "time": evt_time,
                "data": data,
            }));
        }
    }

    let mut out = HashMap::new();

    if let Some(first) = fired.first() {
        out.insert(
            "event".to_string(),
            Message::object(EncodableValue::from(first.clone())),
        );
    }

    if !fired.is_empty() {
        let arr: Vec<EncodableValue> = fired.iter().map(|v| EncodableValue::from(v.clone())).collect();
        out.insert("events".to_string(), Message::Array(Arc::new(arr)));
    }

    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "time": t,
            "firedCount": fired.len(),
        }))),
    );

    Ok(out)
}
