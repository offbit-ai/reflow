//! State machine system — Interactive animation-style state management.
//!
//! State machines are components. The system reads triggers, evaluates
//! transitions, and writes the current state + active animation back.
//!
//! ## Component schema: `entity:state_machine`
//!
//! ```json
//! {
//!   "current": "idle",
//!   "states": {
//!     "idle": {
//!       "animation": "idle_loop",
//!       "loop": true
//!     },
//!     "hover": {
//!       "animation": "hover_in",
//!       "onEnter": { "tween": "scale_up" },
//!       "onExit": { "tween": "scale_down" }
//!     },
//!     "pressed": {
//!       "animation": "press_bounce"
//!     },
//!     "disabled": {
//!       "animation": "fade_out"
//!     }
//!   },
//!   "transitions": [
//!     { "from": "idle", "to": "hover", "trigger": "pointerEnter" },
//!     { "from": "hover", "to": "idle", "trigger": "pointerLeave" },
//!     { "from": "hover", "to": "pressed", "trigger": "pointerDown" },
//!     { "from": "pressed", "to": "hover", "trigger": "pointerUp" },
//!     { "from": "*", "to": "disabled", "trigger": "disable" }
//!   ]
//! }
//! ```
//!
//! ## Component schema: `entity:triggers`
//!
//! ```json
//! ["pointerEnter", "scroll_50"]
//! ```
//!
//! Triggers are consumed each tick. External systems (input, scroll, viewport)
//! write triggers; the state machine reads and clears them.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    StateMachineSystemActor,
    inports::<10>(tick, trigger, entity_id),
    outports::<1>(state_changes, metadata),
    state(MemoryState)
)]
pub async fn scene_state_machine_system_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");

    let global_triggers: Vec<String> = match payload.get("trigger") {
        Some(Message::String(s)) => vec![s.to_string()],
        Some(Message::Object(obj)) => {
            let v: Value = obj.as_ref().clone().into();
            v.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default()
        }
        _ => Vec::new(),
    };

    let db = get_or_create_db(db_path)?;

    let selected = super::selector::resolve_entities(&payload, &config, &db);
    let sm_entries = if selected.is_empty() {
        db.query(&reflow_assets::AssetQuery::new().asset_type("state_machine"))?
    } else {
        selected
            .iter()
            .filter_map(|e| db.get_entry(&format!("{}:state_machine", e)).ok())
            .collect()
    };

    let mut state_changes = Vec::new();
    let mut processed = 0;

    for entry in &sm_entries {
        let sm = match &entry.inline_data {
            Some(v) => v.clone(),
            None => continue,
        };

        let current = sm.get("current").and_then(|v| v.as_str()).unwrap_or("idle");

        // Collect triggers: entity-specific + global
        let entity = &entry.name;
        let mut triggers = global_triggers.clone();

        // Read entity-specific triggers component
        if let Ok(trig_asset) = db.get_component(entity, "triggers") {
            if let Some(ref inline) = trig_asset.entry.inline_data {
                if let Some(arr) = inline.as_array() {
                    for t in arr {
                        if let Some(s) = t.as_str() {
                            triggers.push(s.to_string());
                        }
                    }
                }
            }
            // Consume triggers (clear them)
            let _ = db.set_component_json(entity, "triggers", json!([]), json!({}));
        }

        if triggers.is_empty() {
            continue;
        }

        // Evaluate transitions
        let transitions = sm.get("transitions").and_then(|v| v.as_array());
        let states = sm.get("states");

        let mut new_state = current.to_string();

        if let Some(trans) = transitions {
            for trigger in &triggers {
                for t in trans {
                    let from = t.get("from").and_then(|v| v.as_str()).unwrap_or("");
                    let to = t.get("to").and_then(|v| v.as_str()).unwrap_or("");
                    let t_trigger = t.get("trigger").and_then(|v| v.as_str()).unwrap_or("");
                    let guard = t.get("guard").and_then(|v| v.as_str());

                    // Match: from must be current state or wildcard "*"
                    if t_trigger == trigger && (from == current || from == "*") {
                        // Optional guard: check if a boolean component is true
                        if let Some(guard_path) = guard {
                            if !check_guard(&db, entity, guard_path) {
                                continue;
                            }
                        }

                        new_state = to.to_string();
                        break;
                    }
                }
                if new_state != current {
                    break;
                }
            }
        }

        if new_state != current {
            // Fire onExit for old state
            if let Some(states_obj) = states {
                if let Some(old_state) = states_obj.get(current) {
                    if let Some(on_exit) = old_state.get("onExit") {
                        fire_action(&db, entity, on_exit);
                    }
                }
                // Fire onEnter for new state
                if let Some(new_state_obj) = states_obj.get(&new_state) {
                    if let Some(on_enter) = new_state_obj.get("onEnter") {
                        fire_action(&db, entity, on_enter);
                    }
                }
            }

            state_changes.push(json!({
                "entity": entity,
                "from": current,
                "to": new_state,
            }));

            // Write updated state
            let mut updated = sm.clone();
            updated["current"] = json!(new_state);
            updated["previousState"] = json!(current);
            let _ = db.put_json(&entry.id, updated, entry.metadata.clone());
        }

        processed += 1;
    }

    let mut out = HashMap::new();
    if !state_changes.is_empty() {
        out.insert(
            "state_changes".to_string(),
            Message::object(EncodableValue::from(json!(state_changes))),
        );
    }
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "stateMachinesProcessed": processed,
            "stateChanges": state_changes.len(),
        }))),
    );
    Ok(out)
}

fn check_guard(
    db: &std::sync::Arc<reflow_assets::AssetDB>,
    entity: &str,
    guard_path: &str,
) -> bool {
    // Guard format: "component.field" — checks if the value is truthy
    let parts: Vec<&str> = guard_path.splitn(2, '.').collect();
    let component = parts[0];
    let field = parts.get(1).copied();

    match db.get_component(entity, component) {
        Ok(asset) => {
            let v: Value = if let Some(ref inline) = asset.entry.inline_data {
                inline.clone()
            } else {
                serde_json::from_slice(&asset.data).unwrap_or(Value::Null)
            };

            let target = if let Some(f) = field {
                v.get(f).cloned().unwrap_or(Value::Null)
            } else {
                v
            };

            match target {
                Value::Bool(b) => b,
                Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
                Value::String(s) => !s.is_empty(),
                Value::Null => false,
                _ => true,
            }
        }
        Err(_) => false,
    }
}

/// Fire an action from onEnter/onExit.
/// Actions can create/modify components: { "tween": "scale_up" } starts a tween.
fn fire_action(db: &std::sync::Arc<reflow_assets::AssetDB>, entity: &str, action: &Value) {
    if let Some(tween_name) = action.get("tween").and_then(|v| v.as_str()) {
        // Set a tween to "playing" state
        let tween_id = format!("{}:{}", tween_name, "tween");
        if let Ok(asset) = db.get(&tween_id) {
            if let Some(ref inline) = asset.entry.inline_data {
                let mut tween = inline.clone();
                tween["state"] = json!("playing");
                tween["elapsed"] = json!(0.0);
                let _ = db.put_json(&tween_id, tween, asset.entry.metadata.clone());
            }
        }
    }

    if let Some(set_component) = action.get("set") {
        // Directly set a component value
        if let Some(obj) = set_component.as_object() {
            for (comp, value) in obj {
                let _ = db.set_component_json(entity, comp, value.clone(), json!({}));
            }
        }
    }
}
