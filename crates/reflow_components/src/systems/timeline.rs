//! Timeline system — multi-track keyframe sequencer.
//!
//! A timeline is a component containing tracks, each targeting a property
//! with keyframes. The system advances the playhead and evaluates keyframes.
//!
//! ## Component schema: `entity:timeline`
//!
//! ```json
//! {
//!   "duration": 3.0,
//!   "playback": "playing",
//!   "speed": 1.0,
//!   "loop": true,
//!   "elapsed": 0.0,
//!   "tracks": [
//!     {
//!       "target": "logo:transform.scale",
//!       "keyframes": [
//!         { "time": 0.0, "value": [0, 0, 0], "easing": "easeOutBack" },
//!         { "time": 0.5, "value": [1, 1, 1] },
//!         { "time": 2.5, "value": [1, 1, 1], "easing": "easeInCubic" },
//!         { "time": 3.0, "value": [0, 0, 0] }
//!       ]
//!     },
//!     {
//!       "target": "title:transform.position",
//!       "delay": 0.3,
//!       "keyframes": [
//!         { "time": 0.0, "value": [0, -2, 0], "easing": "easeOutCubic" },
//!         { "time": 0.8, "value": [0, 0, 0] }
//!       ]
//!     }
//!   ]
//! }
//! ```

use crate::math::easing;
use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    TimelineSystemActor,
    inports::<10>(tick, dt, command, entity_id),
    outports::<1>(events, metadata),
    state(MemoryState)
)]
pub async fn scene_timeline_system_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");
    let dt = match payload.get("dt") {
        Some(Message::Float(f)) => *f,
        _ => config
            .get("dt")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0 / 60.0),
    };

    let db = get_or_create_db(db_path)?;

    let selected = super::selector::resolve_entities(&payload, &config, &db);
    let timelines = if selected.is_empty() {
        db.query(&reflow_assets::AssetQuery::new().asset_type("timeline"))?
    } else {
        selected
            .iter()
            .filter_map(|e| db.get_entry(&format!("{}:timeline", e)).ok())
            .collect()
    };

    let mut events = Vec::new();
    let mut active_count = 0;

    for entry in &timelines {
        let tl = match &entry.inline_data {
            Some(v) => v.clone(),
            None => continue,
        };

        let playback = tl
            .get("playback")
            .and_then(|v| v.as_str())
            .unwrap_or("stopped");
        if playback != "playing" {
            continue;
        }

        let duration = tl.get("duration").and_then(|v| v.as_f64()).unwrap_or(1.0);
        let speed = tl.get("speed").and_then(|v| v.as_f64()).unwrap_or(1.0);
        let do_loop = tl.get("loop").and_then(|v| v.as_bool()).unwrap_or(false);
        let mut elapsed = tl.get("elapsed").and_then(|v| v.as_f64()).unwrap_or(0.0);

        elapsed += dt * speed;

        let mut new_playback = "playing";
        if elapsed >= duration {
            if do_loop {
                elapsed = elapsed % duration;
            } else {
                elapsed = duration;
                new_playback = "completed";
                events.push(json!({ "timeline": entry.id, "event": "completed" }));
            }
        }

        // Evaluate each track
        if let Some(tracks) = tl.get("tracks").and_then(|v| v.as_array()) {
            for track in tracks {
                let target = track.get("target").and_then(|v| v.as_str()).unwrap_or("");
                let track_delay = track.get("delay").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let track_time = elapsed - track_delay;

                if track_time < 0.0 || target.is_empty() {
                    continue;
                }

                if let Some(keyframes) = track.get("keyframes").and_then(|v| v.as_array()) {
                    if let Some(value) = evaluate_keyframes(keyframes, track_time) {
                        write_target(&db, target, &value);
                    }
                }
            }
        }

        // Update timeline state
        let mut updated = tl.clone();
        updated["elapsed"] = json!(elapsed);
        updated["playback"] = json!(new_playback);
        let _ = db.put_json(&entry.id, updated, entry.metadata.clone());

        active_count += 1;
    }

    let mut out = HashMap::new();
    if !events.is_empty() {
        out.insert(
            "events".to_string(),
            Message::object(EncodableValue::from(json!(events))),
        );
    }
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "activeTimelines": active_count,
            "events": events.len(),
        }))),
    );
    Ok(out)
}

fn evaluate_keyframes(keyframes: &[Value], time: f64) -> Option<Value> {
    if keyframes.is_empty() {
        return None;
    }
    if keyframes.len() == 1 {
        return keyframes[0].get("value").cloned();
    }

    // Find surrounding keyframes
    let mut prev_idx = 0;
    let mut next_idx = 0;

    for (i, kf) in keyframes.iter().enumerate() {
        let kf_time = kf.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if kf_time <= time {
            prev_idx = i;
        }
        if kf_time >= time && next_idx <= prev_idx {
            next_idx = i;
        }
    }

    // Before first keyframe
    if next_idx == 0 && prev_idx == 0 {
        return keyframes[0].get("value").cloned();
    }

    // After last keyframe
    if prev_idx == next_idx || prev_idx == keyframes.len() - 1 {
        return keyframes[prev_idx].get("value").cloned();
    }

    // Interpolate between prev and next
    let prev = &keyframes[prev_idx];
    let next = &keyframes[next_idx];

    let prev_time = prev.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let next_time = next.get("time").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let prev_val = prev.get("value")?;
    let next_val = next.get("value")?;

    let segment_duration = next_time - prev_time;
    let t = if segment_duration > 0.0 {
        ((time - prev_time) / segment_duration).clamp(0.0, 1.0)
    } else {
        1.0
    };

    // Easing from the starting keyframe
    let easing_fn = prev
        .get("easing")
        .and_then(|v| v.as_str())
        .unwrap_or("linear");

    Some(interpolate_value(prev_val, next_val, t, easing_fn))
}

fn interpolate_value(from: &Value, to: &Value, t: f64, easing_fn: &str) -> Value {
    match (from, to) {
        (Value::Number(a), Value::Number(b)) => {
            let a = a.as_f64().unwrap_or(0.0);
            let b = b.as_f64().unwrap_or(0.0);
            json!(easing::lerp_eased(a, b, t, easing_fn))
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            let result: Vec<f64> = a
                .iter()
                .zip(b.iter())
                .map(|(av, bv)| {
                    let a = av.as_f64().unwrap_or(0.0);
                    let b = bv.as_f64().unwrap_or(0.0);
                    easing::lerp_eased(a, b, t, easing_fn)
                })
                .collect();
            json!(result)
        }
        _ => {
            if easing::eval(easing_fn, t) >= 0.5 {
                to.clone()
            } else {
                from.clone()
            }
        }
    }
}

fn write_target(db: &std::sync::Arc<reflow_assets::AssetDB>, path: &str, value: &Value) {
    let parts: Vec<&str> = path.splitn(2, '.').collect();
    let entity_component = parts[0];

    if parts.len() == 1 {
        let _ = db.put_json(entity_component, value.clone(), json!({}));
        return;
    }

    let field_path = parts[1];
    if let Ok(asset) = db.get(entity_component) {
        let mut current: Value = if let Some(ref inline) = asset.entry.inline_data {
            inline.clone()
        } else {
            serde_json::from_slice(&asset.data).unwrap_or(json!({}))
        };
        set_json_path(&mut current, field_path, value.clone());
        let _ = db.put_json(entity_component, current, asset.entry.metadata);
    }
}

fn set_json_path(obj: &mut Value, path: &str, value: Value) {
    let keys: Vec<&str> = path.split('.').collect();
    let mut current = obj;
    for (i, key) in keys.iter().enumerate() {
        if i == keys.len() - 1 {
            current[key] = value;
            return;
        }
        if !current.get(key).map(|v| v.is_object()).unwrap_or(false) {
            current[key] = json!({});
        }
        current = &mut current[key];
    }
}
