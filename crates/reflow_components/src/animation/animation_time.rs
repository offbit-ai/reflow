//! Animation time actor — stateful time accumulator for playback.
//!
//! Driven by an IntervalTriggerActor upstream. Each trigger advances the
//! elapsed time by `1/fps * speed`. Outputs current time and frame number.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    AnimationTimeActor,
    inports::<10>(trigger),
    outports::<1>(time, frame_number),
    state(MemoryState)
)]
pub async fn animation_time_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let speed = config
        .get("speed")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let fps = config
        .get("fps")
        .and_then(|v| v.as_f64())
        .unwrap_or(30.0);
    let duration = config
        .get("duration")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0); // 0 = unlimited

    // Read accumulated state from pool
    let state: HashMap<String, serde_json::Value> =
        ctx.get_pool("_anim_time").into_iter().collect();

    let elapsed = state
        .get("elapsed")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let frame = state
        .get("frame")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let dt = (1.0 / fps) * speed;
    let new_elapsed = elapsed + dt;
    let new_frame = frame + 1;

    // Persist state
    ctx.pool_upsert("_anim_time", "elapsed", json!(new_elapsed));
    ctx.pool_upsert("_anim_time", "frame", json!(new_frame));

    if new_frame <= 3 || new_frame % 30 == 0 {
        eprintln!("[AnimTime] frame={} time={:.3}", new_frame, new_elapsed);
    }

    let mut out = HashMap::new();
    out.insert("time".to_string(), Message::Float(new_elapsed));
    out.insert("frame_number".to_string(), Message::Integer(new_frame as i64));
    Ok(out)
}
