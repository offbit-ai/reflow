//! Keyframe actor — standalone single-track keyframe interpolation.
//!
//! A pure DAG actor, not an ECS system. Takes a time input, evaluates
//! keyframes, outputs the interpolated value. No AssetDB dependency.
//! Use for live, on-the-fly, programmable effects.
//!
//! ## Config
//!
//! ```json
//! {
//!   "keyframes": [
//!     { "time": 0.0, "value": [0, 0, 0], "easing": "easeOutCubic" },
//!     { "time": 0.5, "value": [5, 0, 0] },
//!     { "time": 1.0, "value": [5, 3, 0], "easing": "easeInOutQuad" },
//!     { "time": 2.0, "value": [0, 0, 0] }
//!   ],
//!   "loop": false,
//!   "duration": 2.0
//! }
//! ```
//!
//! ## DAG wiring
//!
//! ```text
//! AnimationTime → time → KeyframeActor → value → any target
//! ```
//!
//! Chain multiple for multi-property animation:
//!
//! ```text
//!                      ┌→ Keyframe(position) → position
//! AnimationTime → time ┼→ Keyframe(rotation) → rotation
//!                      └→ Keyframe(scale)    → scale
//! ```

use crate::math::easing;
use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    KeyframeActor,
    inports::<10>(time),
    outports::<1>(value, progress, metadata),
    state(MemoryState),
    await_inports(time)
)]
pub async fn keyframe_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    // Time from inport (guaranteed present via await_inports)
    let time = match payload.get("time") {
        Some(Message::Float(f)) => *f,
        Some(Message::Integer(i)) => *i as f64,
        _ => unreachable!("await_inports guarantees time"),
    };

    let keyframes = config
        .get("keyframes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("KeyframeActor requires keyframes config"))?;

    let duration = config
        .get("duration")
        .and_then(|v| v.as_f64())
        .unwrap_or_else(|| {
            // Auto-detect from last keyframe time
            keyframes
                .last()
                .and_then(|kf| kf.get("time").and_then(|v| v.as_f64()))
                .unwrap_or(1.0)
        });

    let do_loop = config
        .get("loop")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let pingpong = config
        .get("pingpong")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Compute effective time
    let effective_time = if do_loop && duration > 0.0 {
        if pingpong {
            let cycle = time / duration;
            let frac = cycle.fract();
            let is_reverse = (cycle as u64) % 2 == 1;
            if is_reverse {
                (1.0 - frac) * duration
            } else {
                frac * duration
            }
        } else {
            time % duration
        }
    } else {
        time.clamp(0.0, duration)
    };

    let progress = if duration > 0.0 {
        effective_time / duration
    } else {
        1.0
    };

    // Evaluate keyframes
    let value = evaluate_keyframes(keyframes, effective_time);

    let mut out = HashMap::new();
    if let Some(v) = value {
        // Output as the most specific Message type for direct wiring
        let msg = match &v {
            Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    Message::Float(f)
                } else if let Some(i) = n.as_i64() {
                    Message::Integer(i)
                } else {
                    Message::object(EncodableValue::from(v.clone()))
                }
            }
            _ => Message::object(EncodableValue::from(v.clone())),
        };
        out.insert("value".to_string(), msg);
    }
    out.insert("progress".to_string(), Message::Float(progress));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "time": effective_time,
            "progress": progress,
            "duration": duration,
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

    Some(interpolate(pv, nv, t, easing_fn))
}

fn interpolate(from: &Value, to: &Value, t: f64, easing_fn: &str) -> Value {
    let e = easing::eval(easing_fn, t);
    match (from, to) {
        (Value::Number(a), Value::Number(b)) => {
            let a = a.as_f64().unwrap_or(0.0);
            let b = b.as_f64().unwrap_or(0.0);
            json!(a + (b - a) * e)
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            let r: Vec<f64> = a
                .iter()
                .zip(b.iter())
                .map(|(av, bv)| {
                    let a = av.as_f64().unwrap_or(0.0);
                    let b = bv.as_f64().unwrap_or(0.0);
                    a + (b - a) * e
                })
                .collect();
            json!(r)
        }
        _ => {
            if e >= 0.5 {
                to.clone()
            } else {
                from.clone()
            }
        }
    }
}
