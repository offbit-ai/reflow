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
//!   "speed": 1.0,
//!   "stagger": {
//!     "delay": 0.1,
//!     "order": "index",
//!     "from": "start",
//!     "easing": "easeOutCubic"
//!   }
//! }
//! ```
//!
//! ## Stagger
//!
//! Automatically offsets each track's start time. Useful for cascading
//! animations across multiple elements (list items, grid cells, letters).
//!
//! - `delay` — time offset between consecutive tracks (seconds)
//! - `order` — `"index"` (arrival order), `"name"` (alphabetical), `"reverse"`
//! - `from` — `"start"` (first track has zero delay), `"center"`, `"end"`
//! - `easing` — ease the stagger distribution itself (optional)
//!
//! Example: 5 tracks with stagger delay 0.1 from start:
//!   track 0 starts at 0.0s, track 1 at 0.1s, track 2 at 0.2s, ...
//!
//! With `from: "center"`:
//!   track 0 at 0.2s, track 1 at 0.1s, track 2 at 0.0s, track 3 at 0.1s, track 4 at 0.2s
//!
//! ## Control (on `control` inport)
//!
//! Send a string: `"play"`, `"pause"`, `"stop"`, `"reverse"`, `"seek:0.5"`

use crate::math::easing;
use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    AnimationTimelineActor,
    inports::<100>(tick, tracks, control),
    outports::<50>(values, state, progress, metadata),
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

    // ─── Load config tracks into pool on first invocation ───
    if ctx.get_pool("_tracks").is_empty() {
        if let Some(Value::Object(tracks_map)) = config.get("tracks") {
            for (name, track_data) in tracks_map {
                ctx.pool_upsert("_tracks", name, track_data.clone());
            }
        }
    }

    // ─── Pool incoming tracks from inport (override config) ───
    // Each message on `tracks` adds/replaces a named track.
    if let Some(Message::Object(obj)) = payload.get("tracks") {
        let v: Value = obj.as_ref().clone().into();
        let name = v
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();
        ctx.pool_upsert("_tracks", &name, v);
    }

    // ─── Process control ───
    let mut playback_state: String = ctx
        .get_pool("_tl")
        .into_iter()
        .find(|(k, _)| k == "state")
        .and_then(|(_, v)| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| {
            if autoplay {
                "playing".into()
            } else {
                "paused".into()
            }
        });

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
            "stop" => {
                playback_state = "paused".into();
                elapsed = 0.0;
            }
            "reverse" => {
                speed = -speed.abs();
                playback_state = "playing".into();
            }
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
            if do_loop {
                elapsed %= duration;
            } else {
                elapsed = duration;
                playback_state = "completed".into();
            }
        } else if elapsed < 0.0 {
            if do_loop {
                elapsed = duration + (elapsed % duration);
            } else {
                elapsed = 0.0;
                playback_state = "completed".into();
            }
        }
    }

    ctx.pool_upsert("_tl", "state", json!(playback_state));
    ctx.pool_upsert("_tl", "elapsed", json!(elapsed));
    ctx.pool_upsert("_tl", "speed", json!(speed));

    let progress = if duration > 0.0 {
        elapsed / duration
    } else {
        1.0
    };

    // ─── Evaluate all pooled tracks with stagger ───
    let track_pool: Vec<(String, Value)> = ctx.get_pool("_tracks").into_iter().collect();

    let mut out = HashMap::new();

    // Compute stagger delays
    let stagger_delays = compute_stagger(&config, &track_pool);

    for (i, (track_name, track_data)) in track_pool.iter().enumerate() {
        let keyframes = track_data.get("keyframes").and_then(|v| v.as_array());

        if let Some(kf) = keyframes {
            // Per-track delay = explicit delay + stagger offset
            let explicit_delay = track_data
                .get("delay")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let stagger_delay = stagger_delays.get(i).copied().unwrap_or(0.0);
            let track_time = elapsed - explicit_delay - stagger_delay;

            // Before delay: hold at first keyframe value (start state).
            // After delay: interpolate keyframes normally.
            let value = if track_time >= 0.0 {
                evaluate_keyframes(kf, track_time)
            } else {
                kf.first().and_then(|kf0| kf0.get("value").cloned())
            };

            if let Some(value) = value {
                let msg = match &value {
                    Value::Number(n) => {
                        if let Some(f) = n.as_f64() {
                            Message::Float(f)
                        } else if let Some(i) = n.as_i64() {
                            Message::Integer(i)
                        } else {
                            Message::object(EncodableValue::from(value.clone()))
                        }
                    }
                    _ => Message::object(EncodableValue::from(value)),
                };
                out.insert(track_name.clone(), msg);
            }
        }
    }

    // Output all track values as one object on `values` outport
    // so downstream actors get all animated values in one message per tick
    if !out.is_empty() {
        let mut values_obj = serde_json::Map::new();
        for (k, v) in &out {
            match v {
                Message::Float(f) => {
                    values_obj.insert(k.clone(), json!(f));
                }
                Message::Integer(i) => {
                    values_obj.insert(k.clone(), json!(i));
                }
                _ => {}
            }
        }
        if !values_obj.is_empty() {
            out.insert(
                "values".to_string(),
                Message::object(EncodableValue::from(Value::Object(values_obj))),
            );
        }
    }

    out.insert(
        "state".to_string(),
        Message::String(playback_state.clone().into()),
    );
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

/// Compute per-track stagger delays from config.
fn compute_stagger(config: &HashMap<String, Value>, tracks: &[(String, Value)]) -> Vec<f64> {
    let n = tracks.len();
    if n == 0 {
        return Vec::new();
    }

    let stagger = match config.get("stagger") {
        Some(v) if v.is_object() => v,
        // Shorthand: "stagger": 0.1 → delay 0.1 per track
        Some(v) if v.is_number() => {
            let delay = v.as_f64().unwrap_or(0.0);
            return (0..n).map(|i| i as f64 * delay).collect();
        }
        _ => return vec![0.0; n],
    };

    let delay = stagger.get("delay").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if delay == 0.0 {
        return vec![0.0; n];
    }

    let order = stagger
        .get("order")
        .and_then(|v| v.as_str())
        .unwrap_or("index");
    let from = stagger
        .get("from")
        .and_then(|v| v.as_str())
        .unwrap_or("start");
    let stagger_easing = stagger
        .get("easing")
        .and_then(|v| v.as_str())
        .unwrap_or("linear");

    // Build index order
    let mut indices: Vec<usize> = (0..n).collect();
    match order {
        "name" => {
            let mut named: Vec<(usize, &str)> = tracks
                .iter()
                .enumerate()
                .map(|(i, (name, _))| (i, name.as_str()))
                .collect();
            named.sort_by_key(|(_, name)| *name);
            indices = named.iter().map(|(i, _)| *i).collect();
        }
        "reverse" => indices.reverse(),
        _ => {} // "index" = arrival order, already correct
    }

    // Compute raw positions [0..1] for each track in the stagger
    let mut delays = vec![0.0f64; n];
    let max_idx = (n - 1).max(1) as f64;

    for (rank, &original_idx) in indices.iter().enumerate() {
        let normalized = rank as f64 / max_idx; // 0..1

        // Apply "from" mode
        let adjusted = match from {
            "end" => 1.0 - normalized,
            "center" => {
                let center_dist = (normalized - 0.5).abs() * 2.0; // 0 at center, 1 at edges
                center_dist
            }
            "edges" => {
                let center_dist = (normalized - 0.5).abs() * 2.0;
                1.0 - center_dist // 0 at edges, 1 at center
            }
            _ => normalized, // "start"
        };

        // Apply stagger easing
        let eased = easing::eval(stagger_easing, adjusted);
        delays[original_idx] = eased * delay * max_idx;
    }

    delays
}

fn evaluate_keyframes(keyframes: &[Value], time: f64) -> Option<Value> {
    if keyframes.is_empty() {
        return None;
    }
    if keyframes.len() == 1 {
        return keyframes[0].get("value").cloned();
    }

    let mut prev_idx = 0;
    let mut next_idx = keyframes.len() - 1;

    for (i, kf) in keyframes.iter().enumerate() {
        let kt = kf.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if kt <= time {
            prev_idx = i;
        }
        if kt >= time && i > prev_idx {
            next_idx = i;
            break;
        }
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
    let easing_fn = prev
        .get("easing")
        .and_then(|v| v.as_str())
        .unwrap_or("linear");

    let seg = nt - pt;
    let t = if seg > 0.0 {
        ((time - pt) / seg).clamp(0.0, 1.0)
    } else {
        1.0
    };
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
            let r: Vec<f64> = a
                .iter()
                .zip(b.iter())
                .map(|(av, bv)| {
                    let a = av.as_f64().unwrap_or(0.0);
                    let b = bv.as_f64().unwrap_or(0.0);
                    a + (b - a) * t
                })
                .collect();
            json!(r)
        }
        _ => {
            if t >= 0.5 {
                to.clone()
            } else {
                from.clone()
            }
        }
    }
}
