//! Behavior system — persistent expression-driven rules attached to entities.
//!
//! A behavior is a set of rules that continuously drive properties based on
//! expressions and conditions. Unlike one-shot DAG math, behaviors persist
//! in the AssetDB and evaluate every tick — they ARE the entity's logic.
//!
//! ## Component schema: `entity:behavior`
//!
//! ```json
//! {
//!   "rules": [
//!     {
//!       "name": "spin",
//!       "target": "transform.rotation.y",
//!       "expr": "time * 90",
//!       "enabled": true
//!     },
//!     {
//!       "name": "pulse_glow",
//!       "target": "material.emissiveStrength",
//!       "expr": "abs(sin(time * 3)) * intensity",
//!       "when": "alert_level > 0",
//!       "vars": { "intensity": 2.0 },
//!       "enabled": true
//!     },
//!     {
//!       "name": "hover_bob",
//!       "target": "transform.position.y",
//!       "expr": "base_y + sin(time * 2) * amplitude",
//!       "vars": {
//!         "base_y": 2.0,
//!         "amplitude": "config:hover.amplitude"
//!       }
//!     },
//!     {
//!       "name": "face_player",
//!       "target": "transform.rotation.y",
//!       "expr": "atan(pz - oz, px - ox)",
//!       "vars": {
//!         "px": "player:transform.position.0",
//!         "pz": "player:transform.position.2",
//!         "ox": "self:transform.position.0",
//!         "oz": "self:transform.position.2"
//!       }
//!     },
//!     {
//!       "name": "damage_flash",
//!       "target": "material.albedo",
//!       "expr": "[lerp(base_r, 1, flash), lerp(base_g, 0, flash), lerp(base_b, 0, flash)]",
//!       "when": "flash > 0",
//!       "vars": {
//!         "flash": "self:damage.flash",
//!         "base_r": 0.8, "base_g": 0.2, "base_b": 0.1
//!       }
//!     }
//!   ]
//! }
//! ```
//!
//! ## Rule fields
//!
//! - `name` — identifier for debugging/toggling
//! - `target` — property path relative to the entity (e.g. `transform.position.y`)
//! - `expr` — math expression (uses shared `math::expression` evaluator)
//! - `when` — optional condition expression; rule only fires when this evaluates > 0
//! - `vars` — variable bindings: literal numbers or AssetDB paths
//!   - `"self:component.field"` resolves to the owning entity's own component
//!   - `"entity:component.field"` resolves to another entity
//!   - `2.0` — literal constant
//! - `enabled` — toggle rule on/off (default true)
//!
//! ## Built-in variables (injected automatically)
//!
//! - `time` — elapsed seconds since system start
//! - `dt` — delta time this tick
//! - `frame` — tick counter

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use crate::math::expression as expr_eval;
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);

#[actor(
    BehaviorSystemActor,
    inports::<10>(tick, dt, entity_id),
    outports::<1>(metadata),
    state(MemoryState)
)]
pub async fn behavior_system_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config.get("$db").and_then(|v| v.as_str()).unwrap_or("./assets.db");
    let dt = match payload.get("dt") {
        Some(Message::Float(f)) => *f,
        _ => config.get("dt").and_then(|v| v.as_f64()).unwrap_or(1.0 / 60.0),
    };

    // Optional: target a specific entity (from inport or config)
    let target_entity = match payload.get("entity_id") {
        Some(Message::String(s)) => Some(s.to_string()),
        _ => config.get("entity").and_then(|v| v.as_str()).map(|s| s.to_string()),
    };

    let start = START_TIME.get_or_init(Instant::now);
    let time = start.elapsed().as_secs_f64();
    let frame = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);

    let db = get_or_create_db(db_path)?;

    // If a specific entity is targeted, only process that one
    let behavior_entries = if let Some(ref entity) = target_entity {
        match db.get_entry(&format!("{}:behavior", entity)) {
            Ok(entry) => vec![entry],
            Err(_) => Vec::new(),
        }
    } else {
        db.query(&reflow_assets::AssetQuery::new().asset_type("behavior"))?
    };

    let mut rules_fired = 0u64;
    let mut rules_skipped = 0u64;
    let mut entities_processed = 0u64;

    for entry in &behavior_entries {
        let data = match &entry.inline_data {
            Some(v) => v.clone(),
            None => continue,
        };

        let entity = &entry.name; // entity name from "entity:behavior"
        let rules = match data.get("rules").and_then(|v| v.as_array()) {
            Some(r) => r,
            None => continue,
        };

        entities_processed += 1;

        for rule in rules {
            let enabled = rule.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            if !enabled {
                rules_skipped += 1;
                continue;
            }

            let expr_str = match rule.get("expr").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => continue,
            };
            let target = match rule.get("target").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => continue,
            };

            // Build variable context
            let mut vars: HashMap<String, f64> = HashMap::new();
            vars.insert("time".to_string(), time);
            vars.insert("dt".to_string(), dt);
            vars.insert("frame".to_string(), frame as f64);

            // User-defined vars
            if let Some(var_map) = rule.get("vars").and_then(|v| v.as_object()) {
                for (name, val) in var_map {
                    if let Some(path) = val.as_str() {
                        // Resolve "self:" prefix to the owning entity
                        let resolved_path = if let Some(rest) = path.strip_prefix("self:") {
                            format!("{}:{}", entity, rest)
                        } else {
                            path.to_string()
                        };
                        if let Some(v) = resolve_path(&db, &resolved_path) {
                            vars.insert(name.clone(), v);
                        }
                    } else if let Some(n) = val.as_f64() {
                        vars.insert(name.clone(), n);
                    }
                }
            }

            // Evaluate condition (if present)
            if let Some(when_expr) = rule.get("when").and_then(|v| v.as_str()) {
                match expr_eval::eval(when_expr, &vars) {
                    Some(v) if v > 0.0 => {} // condition met, proceed
                    _ => {
                        rules_skipped += 1;
                        continue;
                    }
                }
            }

            // Evaluate the expression
            // Support array expressions: "[expr1, expr2, expr3]"
            let trimmed = expr_str.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                // Array expression — evaluate each element
                let inner = &trimmed[1..trimmed.len() - 1];
                let parts: Vec<&str> = split_top_level(inner);
                let values: Vec<f64> = parts
                    .iter()
                    .filter_map(|p| expr_eval::eval(p.trim(), &vars))
                    .collect();
                if !values.is_empty() {
                    let full_target = format!("{}:{}", entity, target);
                    write_target(&db, &full_target, &json!(values));
                    rules_fired += 1;
                }
            } else {
                // Scalar expression
                if let Some(result) = expr_eval::eval(expr_str, &vars) {
                    let full_target = format!("{}:{}", entity, target);
                    write_target(&db, &full_target, &json!(result));
                    rules_fired += 1;
                }
            }
        }
    }

    let mut out = HashMap::new();
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "entitiesProcessed": entities_processed,
            "rulesFired": rules_fired,
            "rulesSkipped": rules_skipped,
            "time": time,
            "frame": frame,
        }))),
    );
    Ok(out)
}

/// Split a comma-separated string respecting parentheses depth.
/// "a + b, sin(c, d), e" → ["a + b", "sin(c, d)", "e"]
fn split_top_level(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        parts.push(&s[start..]);
    }
    parts
}

fn resolve_path(db: &std::sync::Arc<reflow_assets::AssetDB>, path: &str) -> Option<f64> {
    let parts: Vec<&str> = path.splitn(2, '.').collect();
    let entity_component = parts[0];
    let field_path = parts.get(1).copied();

    let asset = db.get(entity_component).ok()?;
    let v: Value = if let Some(ref inline) = asset.entry.inline_data {
        inline.clone()
    } else {
        serde_json::from_slice(&asset.data).ok()?
    };

    let target = if let Some(fp) = field_path {
        let mut current = &v;
        for key in fp.split('.') {
            // Support array index access: "position.0", "position.1"
            if let Ok(idx) = key.parse::<usize>() {
                current = current.get(idx)?;
            } else {
                current = current.get(key)?;
            }
        }
        current.clone()
    } else {
        v
    };

    target.as_f64()
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
            // Support array index: "position.1"
            if let Ok(idx) = key.parse::<usize>() {
                if let Value::Array(ref mut arr) = current {
                    if idx < arr.len() {
                        arr[idx] = value;
                        return;
                    }
                }
            }
            current[key] = value;
            return;
        }
        if let Ok(idx) = key.parse::<usize>() {
            current = &mut current[idx];
        } else {
            if !current.get(key).map(|v| v.is_object() || v.is_array()).unwrap_or(false) {
                current[key] = json!({});
            }
            current = &mut current[key];
        }
    }
}
