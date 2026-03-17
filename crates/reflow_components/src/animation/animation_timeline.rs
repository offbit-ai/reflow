//! Animation timeline actor — pools keyframe tracks from the DAG.
//!
//! Wire multiple KeyframeActors (or raw track data) into the `tracks`
//! inport. Each connection adds a named track. The timeline plays them
//! all in sync.
//!
//! ```text
//! Keyframe("position", kf:[...]) ──→ tracks ─┐
//! Keyframe("opacity", kf:[...])  ──→ tracks ─┼→ AnimationTimeline → position
//! Keyframe("scale", kf:[...])    ──→ tracks ─┘                   → opacity
//!                                                                 → scale
//! IntervalTrigger ──→ tick ─────────→
//! ```
//!
//! ## Track input format (on `tracks` inport)
//!
//! ```json
//! {
//!   "name": "position",
//!   "keyframes": [
//!     { "time": 0.0, "value": [0, 0, 0], "easing": "easeOutCubic" },
//!     { "time": 1.0, "value": [5, 3, 0] }
//!   ]
//! }
//! ```
//!
//! Multiple tracks accumulate — each new `tracks` message with a different
//! `name` adds to the pool. Same name replaces (hot-reload keyframes).
//!
//! ## Config
//!
//! ```json
//! {
//!   "duration": 2.0,
//!   "loop": false,
//!   "autoplay": true,
//!   "speed": 1.0
//! }
//! ```
//!
//! ## Control (on `control` inport)
//!
//! Send a string: `"play"`, `"pause"`, `"stop"`, `"reverse"`, `"seek:0.5"`

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use crate::math::easing;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    AnimationTimelineActor,
    inports::<100>(tick, tracks, control),
    outports::<50>(state, progress, metadata),
    state(MemoryState)
)]
pub async fn animation_timeline_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let duration = config
        .get("duration")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let do_loop = config
        .get("loop")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let autoplay = config
        .get("autoplay")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let dt = config
        .get("dt")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0 / 60.0);

    // ─── Pool incoming tracks ───
    // Each message on `tracks` adds/replaces a named track.
    if let Some(Message::Object(obj)) = payload.get("tracks") {
        let v: Value = obj.as_ref().clone().into();
        let name = v.get("name").and_then(|v| v.as_str()).unwrap_or("default").to_string();
        ctx.pool_upsert("_tracks", &name, v);
    }

    // ─── Process control ───
    let mut playback_state: String = ctx
        .get_pool("_tl")
        .into_iter()
        .find(|(k, _)| k == "state")
        .and_then(|(_, v)| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| if autoplay { "playing".into() } else { "paused".into() });

    let mut elapsed: f64 = ctx
        .get_pool("_tl")
        .into_iter()
        .find(|(k, _)| k == "elapsed")
        .and_then(|(_, v)| v.as_f64())
        .unwrap_or(0.0);

    let mut speed: f64 = ctx
        .get_pool("_tl")
        .into_iter()
        .find(|(k, _)| k == "speed")
        .and_then(|(_, v)| v.as_f64())
        .unwrap_or(config.get("speed").and_then(|v| v.as_f64()).unwrap_or(1.0));

    if let Some(Message::String(cmd)) = payload.get("control") {
        let cmd = cmd.to_string();
        match cmd.as_str() {
            "play" => playback_state = "playing".into(),
            "pause" => playback_state = "paused".into(),
            "stop" => { playback_state = "paused".into(); elapsed = 0.0; }
            "reverse" => { speed = -speed.abs(); playback_state = "playing".into(); }
            _ if cmd.starts_with("seek:") => {
                if let Ok(t) = cmd[5..].trim().parse::<f64>() {
                    elapsed = t.clamp(0.0, duration);
                }
            }
            _ => {}
        }
    }
    // Also accept Flow on play/control to start
    if let Some(Message::Flow) = payload.get("control") {
        playback_state = "playing".into();
    }

    // ─── Advance time ───
    if playback_state == "playing" && payload.contains_key("tick") {
        elapsed += dt * speed;

        if elapsed >= duration {
            if do_loop { elapsed %= duration; }
            else { elapsed = duration; playback_state = "completed".into(); }
        } else if elapsed < 0.0 {
            if do_loop { elapsed = duration + (elapsed % duration); }
            else { elapsed = 0.0; playback_state = "completed".into(); }
        }
    }

    ctx.pool_upsert("_tl", "state", json!(playback_state));
    ctx.pool_upsert("_tl", "elapsed", json!(elapsed));
    ctx.pool_upsert("_tl", "speed", json!(speed));

    let progress = if duration > 0.0 { elapsed / duration } else { 1.0 };

    // ─── Evaluate all pooled tracks ───
    let track_pool: Vec<(String, Value)> = ctx.get_pool("_tracks").into_iter().collect();
    let mut out = HashMap::new();

    for (track_name, track_data) in &track_pool {
        let keyframes = track_data
            .get("keyframes")
            .and_then(|v| v.as_array());

        if let Some(kf) = keyframes {
            let delay = track_data.get("delay").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let track_time = elapsed - delay;

            if track_time >= 0.0 {
                if let Some(value) = evaluate_keyframes(kf, track_time) {
                    out.insert(
                        track_name.clone(),
                        Message::object(EncodableValue::from(value)),
                    );
                }
            }
        }
    }

    out.insert("state".to_string(), Message::String(playback_state.clone().into()));
    out.insert("progress".to_string(), Message::Float(progress));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "state": playback_state,
            "elapsed": elapsed,
            "progress": progress,
            "speed": speed,
            "trackCount": track_pool.len(),
            "trackNames": track_pool.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        }))),
    );
    Ok(out)
}

fn evaluate_keyframes(keyframes: &[Value], time: f64) -> Option<Value> {
    if keyframes.is_empty() { return None; }
    if keyframes.len() == 1 { return keyframes[0].get("value").cloned(); }

    let mut prev_idx = 0;
    let mut next_idx = keyframes.len() - 1;

    for (i, kf) in keyframes.iter().enumerate() {
        let kt = kf.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if kt <= time { prev_idx = i; }
        if kt >= time && i > prev_idx { next_idx = i; break; }
    }

    if prev_idx == next_idx {
        return keyframes[prev_idx].get("value").cloned();
    }

    let prev = &keyframes[prev_idx];
    let next = &keyframes[next_idx];
    let pt = prev.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let nt = next.get("time").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let pv = prev.get("value")?;
    let nv = next.get("value")?;
    let easing_fn = prev.get("easing").and_then(|v| v.as_str()).unwrap_or("linear");

    let seg = nt - pt;
    let t = if seg > 0.0 { ((time - pt) / seg).clamp(0.0, 1.0) } else { 1.0 };
    let e = easing::eval(easing_fn, t);

    Some(interpolate(pv, nv, e))
}

fn interpolate(from: &Value, to: &Value, t: f64) -> Value {
    match (from, to) {
        (Value::Number(a), Value::Number(b)) => {
            let a = a.as_f64().unwrap_or(0.0);
            let b = b.as_f64().unwrap_or(0.0);
            json!(a + (b - a) * t)
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            let r: Vec<f64> = a.iter().zip(b.iter()).map(|(av, bv)| {
                let a = av.as_f64().unwrap_or(0.0);
                let b = bv.as_f64().unwrap_or(0.0);
                a + (b - a) * t
            }).collect();
            json!(r)
        }
        _ => if t >= 0.5 { to.clone() } else { from.clone() },
    }
}
