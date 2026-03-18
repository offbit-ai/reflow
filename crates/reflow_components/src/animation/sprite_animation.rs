//! Sprite animation actor — outputs frame indices for sprite sheet playback.
//!
//! Pure DAG actor. Wire time/tick and sprite sheet config, get frame index out.
//!
//! ## Inports (all optional — config provides defaults)
//!
//! - `tick` — advance one frame per invocation
//! - `time` — explicit time in seconds (overrides tick counting)
//! - `sprite` — sprite sheet descriptor: `{ frameCount, fps, startFrame, endFrame, loop, pingpong }`
//!
//! ## Config (defaults when inports not connected)
//!
//! ```json
//! {
//!   "frameCount": 8,
//!   "fps": 12,
//!   "startFrame": 0,
//!   "endFrame": 7,
//!   "loop": true,
//!   "pingpong": false
//! }
//! ```
//!
//! ## DAG wiring
//!
//! Simple (tick-driven, config-based):
//! ```text
//! IntervalTrigger → tick → SpriteAnimation → frame
//! ```
//!
//! Dynamic (swap sprite sheet at runtime):
//! ```text
//! AssetLoad("walk:sprite") → sprite ─┐
//!                                     ├→ SpriteAnimation → frame
//! IntervalTrigger ──────→ tick ──────┘
//! ```
//!
//! Time-driven:
//! ```text
//! AnimationTime → time → SpriteAnimation → frame
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    SpriteAnimationActor,
    inports::<10>(tick, time, sprite),
    outports::<1>(frame, progress, metadata),
    state(MemoryState)
)]
pub async fn sprite_animation_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    // Cache sprite descriptor from inport (persists across ticks)
    if let Some(Message::Object(obj)) = payload.get("sprite") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_sprite", "desc", v);
    }

    // Merge: inport sprite overrides config
    let sprite_desc: HashMap<String, Value> = {
        let cached = ctx
            .get_pool("_sprite")
            .into_iter()
            .find(|(k, _)| k == "desc")
            .map(|(_, v)| v);

        let mut merged = config.clone();
        if let Some(Value::Object(map)) = cached {
            for (k, v) in map {
                merged.insert(k, v);
            }
        }
        merged
    };

    let frame_count = sprite_desc
        .get("frameCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(4) as usize;
    let sprite_fps = sprite_desc
        .get("fps")
        .and_then(|v| v.as_f64())
        .unwrap_or(12.0);
    let start_frame = sprite_desc
        .get("startFrame")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let end_frame = sprite_desc
        .get("endFrame")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(frame_count.saturating_sub(1));
    let do_loop = sprite_desc
        .get("loop")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let pingpong = sprite_desc
        .get("pingpong")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let active_frames = if end_frame >= start_frame {
        end_frame - start_frame + 1
    } else {
        1
    };

    // Build sequence
    let sequence: Vec<usize> = if pingpong && active_frames > 2 {
        let mut seq: Vec<usize> = (start_frame..=end_frame).collect();
        let reverse: Vec<usize> = (start_frame + 1..end_frame).rev().collect();
        seq.extend(reverse);
        seq
    } else {
        (start_frame..=end_frame).collect()
    };
    let seq_len = sequence.len().max(1);

    // Get time: explicit inport, or tick-based counter
    let time = if let Some(Message::Float(t)) = payload.get("time") {
        *t
    } else if let Some(Message::Integer(t)) = payload.get("time") {
        *t as f64
    } else {
        let tick_count = ctx
            .get_pool("_sprite")
            .into_iter()
            .find(|(k, _)| k == "ticks")
            .and_then(|(_, v)| v.as_f64())
            .unwrap_or(0.0)
            + 1.0;
        ctx.pool_upsert("_sprite", "ticks", json!(tick_count));
        tick_count / sprite_fps
    };

    // Compute frame
    let frame_duration = 1.0 / sprite_fps;
    let total_seq_duration = seq_len as f64 * frame_duration;

    let effective_time = if do_loop && total_seq_duration > 0.0 {
        time % total_seq_duration
    } else {
        time.min(total_seq_duration)
    };

    let seq_idx = ((effective_time / frame_duration) as usize).min(seq_len - 1);
    let current_frame = sequence[seq_idx];

    let progress = if total_seq_duration > 0.0 {
        effective_time / total_seq_duration
    } else {
        0.0
    };

    let mut out = HashMap::new();
    out.insert("frame".to_string(), Message::Integer(current_frame as i64));
    out.insert("progress".to_string(), Message::Float(progress));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "frame": current_frame,
            "sequenceLength": seq_len,
            "progress": progress,
            "time": effective_time,
        }))),
    );
    Ok(out)
}
