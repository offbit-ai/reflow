//! Tween system — interpolates properties between values over time.
//!
//! Tweens are components, not actors. The system reads all `:tween`
//! components, advances them, and writes interpolated values to
//! their target components.
//!
//! ## Component schema: `entity:tween`
//!
//! ```json
//! {
//!   "target": "logo:transform.position",
//!   "from": [0, 0, 0],
//!   "to": [5, 0, 0],
//!   "duration": 1.0,
//!   "easing": "easeOutCubic",
//!   "delay": 0.0,
//!   "loop": false,
//!   "yoyo": false,
//!   "elapsed": 0.0,
//!   "state": "playing"
//! }
//! ```
//!
//! Multiple tweens per entity: use `entity:tween`, `entity:tween_2`, etc.
//! Or different entities targeting the same property.

use crate::math::easing;
use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    TweenSystemActor,
    inports::<10>(tick, dt, entity_id),
    outports::<1>(completed, metadata),
    state(MemoryState)
)]
pub async fn scene_tween_system_actor(
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
    let cache = if selected.is_empty() {
        db.query(&reflow_assets::AssetQuery::new().asset_type("tween"))?
    } else {
        selected
            .iter()
            .filter_map(|e| db.get_entry(&format!("{}:tween", e)).ok())
            .collect()
    };

    let mut completed = Vec::new();
    let mut active_count = 0;

    for entry in &cache {
        let tween_data = if let Some(ref inline) = entry.inline_data {
            inline.clone()
        } else {
            continue;
        };

        let state = tween_data
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("playing");
        if state != "playing" {
            continue;
        }

        let duration = tween_data
            .get("duration")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);
        let delay = tween_data
            .get("delay")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let easing_fn = tween_data
            .get("easing")
            .and_then(|v| v.as_str())
            .unwrap_or("linear");
        let do_loop = tween_data
            .get("loop")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let yoyo = tween_data
            .get("yoyo")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut elapsed = tween_data
            .get("elapsed")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        elapsed += dt;

        let active_time = elapsed - delay;
        if active_time < 0.0 {
            // Still in delay
            let mut updated = tween_data.clone();
            updated["elapsed"] = json!(elapsed);
            let _ = db.put_json(&entry.id, updated, entry.metadata.clone());
            continue;
        }

        let mut progress = if duration > 0.0 {
            active_time / duration
        } else {
            1.0
        };
        let mut new_state = "playing";

        if progress >= 1.0 {
            if do_loop {
                if yoyo {
                    // Yoyo: reverse direction each cycle
                    let cycle = (active_time / duration) as u64;
                    let frac = (active_time / duration).fract();
                    progress = if cycle % 2 == 0 { frac } else { 1.0 - frac };
                } else {
                    progress = (active_time / duration).fract();
                }
            } else {
                progress = 1.0;
                new_state = "completed";
                completed.push(entry.id.clone());
            }
        }

        // Evaluate easing
        let from = tween_data.get("from");
        let to = tween_data.get("to");
        let target_path = tween_data
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if let (Some(from), Some(to)) = (from, to) {
            let interpolated = interpolate_value(from, to, progress, easing_fn);
            write_target(&db, target_path, &interpolated);
        }

        // Update tween state
        let mut updated = tween_data.clone();
        updated["elapsed"] = json!(elapsed);
        updated["state"] = json!(new_state);
        updated["progress"] = json!(progress);
        let _ = db.put_json(&entry.id, updated, entry.metadata.clone());

        active_count += 1;
    }

    let mut out = HashMap::new();
    if !completed.is_empty() {
        out.insert(
            "completed".to_string(),
            Message::object(EncodableValue::from(json!(completed))),
        );
    }
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "activeTweens": active_count,
            "completedThisTick": completed.len(),
        }))),
    );
    Ok(out)
}

fn interpolate_value(from: &Value, to: &Value, t: f64, easing_fn: &str) -> Value {
    match (from, to) {
        // Scalar
        (Value::Number(a), Value::Number(b)) => {
            let a = a.as_f64().unwrap_or(0.0);
            let b = b.as_f64().unwrap_or(0.0);
            json!(easing::lerp_eased(a, b, t, easing_fn))
        }
        // Vec (2/3/4)
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
        // Color (object with r,g,b)
        (Value::Object(a), Value::Object(b)) => {
            let mut result = serde_json::Map::new();
            for (key, av) in a {
                if let Some(bv) = b.get(key) {
                    let interp = interpolate_value(av, bv, t, easing_fn);
                    result.insert(key.clone(), interp);
                } else {
                    result.insert(key.clone(), av.clone());
                }
            }
            Value::Object(result)
        }
        _ => to.clone(),
    }
}

/// Write an interpolated value to a target path like "entity:component.field".
fn write_target(db: &std::sync::Arc<reflow_assets::AssetDB>, path: &str, value: &Value) {
    // Parse "entity:component.field.subfield"
    let parts: Vec<&str> = path.splitn(2, '.').collect();
    let entity_component = parts[0];

    if parts.len() == 1 {
        // Direct component write (rare — usually you target a field)
        let _ = db.put_json(entity_component, value.clone(), json!({}));
        return;
    }

    let field_path = parts[1];

    // Read current component, patch the field, write back
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
