//! Hit test actor — detects overlap between two animated shapes.
//!
//! Reads animated position values from the timeline and checks if a
//! source point (e.g., cursor) is inside a target rect (e.g., button).
//! Emits `enter` and `leave` events on state changes, plus forwards
//! `click` when a click signal arrives while inside.
//!
//! ## Config
//!
//! ```json
//! {
//!   "source": "s2",
//!   "target": "s0",
//!   "target_width": 200,
//!   "target_height": 52
//! }
//! ```
//!
//! Source and target are shape prefixes (sN_x, sN_y from timeline values).
//! The hit test checks if the source point is within target bounds.
//!
//! ## Inports
//! - `values` — animated values object (from timeline, same as renderer)
//!
//! ## Outports
//! - `enter` — String "HOVER" emitted when source enters target
//! - `leave` — String "LEAVE" emitted when source leaves target
//! - `inside` — Boolean, current overlap state per tick
//!
//! ## DAG wiring
//!
//! ```text
//! timeline:values ──→ hit_test:values
//! timeline:values ──→ renderer:values
//!
//! hit_test:enter ──→ fsm:event   (HOVER)
//! hit_test:leave ──→ fsm:event   (LEAVE)
//! hit_test:click ──→ fsm:event   (PRESS)
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::ActorContext;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    HitTestActor,
    inports::<100>(values),
    outports::<50>(enter, leave),
    state(MemoryState)
)]
pub async fn hit_test_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let source = config
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("s2");
    let target = config
        .get("target")
        .and_then(|v| v.as_str())
        .unwrap_or("s0");
    let target_w = config
        .get("target_width")
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0);
    let target_h = config
        .get("target_height")
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0);

    // Pool animated values from timeline
    if let Some(Message::Object(obj)) = payload.get("values") {
        let v: Value = obj.as_ref().clone().into();
        if let Some(map) = v.as_object() {
            for (k, val) in map {
                if let Some(f) = val.as_f64() {
                    ctx.pool_upsert("_ht_vals", k, json!(f));
                }
            }
        }
    }

    // Read positions from pool
    let get_val = |prefix: &str, prop: &str| -> f64 {
        ctx.get_pool("_ht_vals")
            .into_iter()
            .find(|(k, _)| k == &format!("{}_{}", prefix, prop))
            .and_then(|(_, v)| v.as_f64())
            .unwrap_or(0.0)
    };

    let src_x = get_val(source, "x");
    let src_y = get_val(source, "y");
    let tgt_x = get_val(target, "x");
    let tgt_y = get_val(target, "y");
    let tgt_scale = ctx
        .get_pool("_ht_vals")
        .into_iter()
        .find(|(k, _)| k == &format!("{}_scale", target))
        .and_then(|(_, v)| v.as_f64())
        .unwrap_or(1.0);

    // Target bounds (centered at tgt_x, tgt_y, scaled)
    let hw = (target_w * tgt_scale) / 2.0;
    let hh = (target_h * tgt_scale) / 2.0;
    let inside = src_x >= tgt_x - hw
        && src_x <= tgt_x + hw
        && src_y >= tgt_y - hh
        && src_y <= tgt_y + hh;

    // Previous state (initialize on first run)
    let pool: Vec<(String, Value)> = ctx.get_pool("_ht").into_iter().collect();
    let initialized = pool.iter().any(|(k, _)| k == "inside");
    let was_inside = pool
        .into_iter()
        .find(|(k, _)| k == "inside")
        .and_then(|(_, v)| v.as_bool())
        .unwrap_or(false);

    if !initialized {
        ctx.pool_upsert("_ht", "inside", json!(false));
        return Ok(HashMap::new());
    }

    // Only emit on state transitions — silent when unchanged
    if inside == was_inside {
        return Ok(HashMap::new());
    }

    ctx.pool_upsert("_ht", "inside", json!(inside));

    let mut out = HashMap::new();
    if inside {
        out.insert(
            "enter".to_string(),
            Message::String("HOVER".to_string().into()),
        );
    } else {
        out.insert(
            "leave".to_string(),
            Message::String("LEAVE".to_string().into()),
        );
    }

    Ok(out)
}
