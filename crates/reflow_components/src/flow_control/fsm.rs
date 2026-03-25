//! Finite State Machine actor — config-driven discrete state logic.
//!
//! Use FSM when DAG is overkill: entity lifecycle, UI screens, animation
//! state machines, game AI modes. The FSM complements DAG data-flow —
//! FSM handles *control flow* (what mode am I in), DAG handles *data flow*
//! (how data transforms and moves).
//!
//! ## Config
//!
//! ```json
//! {
//!   "initial": "idle",
//!   "states": {
//!     "idle": {
//!       "on": {
//!         "START": { "target": "running", "guard": "hasTarget" }
//!       },
//!       "entry": { "emit": { "speed": 0.0 } }
//!     },
//!     "running": {
//!       "on": {
//!         "STOP": { "target": "idle" },
//!         "HIT":  { "target": "hit" }
//!       },
//!       "entry": { "emit": { "speed": 1.0 } }
//!     },
//!     "hit": {
//!       "on": { "_timeout": { "target": "running", "delay": 0.5 } },
//!       "entry": {
//!         "emit": { "flash": true },
//!         "assign": { "lives": { "op": "decrement" } }
//!       }
//!     },
//!     "done": { "type": "final" }
//!   },
//!   "guards": {
//!     "hasTarget": { "field": "target", "operator": "not_empty" }
//!   },
//!   "context": { "score": 0, "lives": 3 }
//! }
//! ```
//!
//! ## Transient states
//!
//! States with `"always"` transitions evaluate immediately on entry —
//! no event needed. Use for decision nodes and chained routing:
//!
//! ```json
//! "check_lives": {
//!   "always": [
//!     { "target": "dead", "guard": "noLives" },
//!     { "target": "respawn" }
//!   ]
//! }
//! ```
//!
//! The first matching guard wins. A guardless entry is the default/fallback.
//! Transient states chain (A → transient B → transient C → D) with a max
//! depth of 10 to prevent infinite loops.
//!
//! ## Guard evaluation
//!
//! Guards evaluate against a merged view of event payload + FSM context.
//! Same operators as ConditionalBranchActor: `is`, `is_not`, `contains`,
//! `not_contains`, `greater_than`, `less_than`, `empty`, `not_empty`, etc.
//!
//! ## Inports
//!
//! - `event` — event name (String) or `{ "type": "NAME", "payload": {...} }`
//! - `tick` — clock for `_timeout` transitions (from IntervalTrigger)
//! - `control` — `"reset"` or `"set:stateName"`
//! - `data` — inject/update context without triggering transition
//!
//! ## Outports
//!
//! - `state` — current state name (String, on every transition)
//! - `transition` — `{ "from": "...", "to": "...", "event": "..." }`
//! - `context` — full context snapshot (when entry has `"emit_context": true`)
//! - `done` — Flow signal when a `final` state is reached
//! - `emit` — merged entry/exit emit values for downstream actors

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

const MAX_TRANSIENT_DEPTH: usize = 10;

#[actor(
    FsmActor,
    inports::<100>(event, tick, control, data),
    outports::<50>(state, transition, context, done, emit),
    state(MemoryState)
)]
pub async fn fsm_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let states = config
        .get("states")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));
    let guards_def = config
        .get("guards")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));
    let initial = config
        .get("initial")
        .and_then(|v| v.as_str())
        .unwrap_or("initial")
        .to_string();

    // ─── Load/init context ───
    let stored_ctx: Vec<(String, Value)> = ctx.get_pool("_fsm_ctx").into_iter().collect();
    if stored_ctx.is_empty() {
        if let Some(Value::Object(init_ctx)) = config.get("context") {
            for (k, v) in init_ctx {
                ctx.pool_upsert("_fsm_ctx", k, v.clone());
            }
        }
    }

    // ─── Current state ───
    let fsm_pool: Vec<(String, Value)> = ctx.get_pool("_fsm").into_iter().collect();
    let first_run = !fsm_pool.iter().any(|(k, _)| k == "current");
    let current_state = fsm_pool
        .into_iter()
        .find(|(k, _)| k == "current")
        .and_then(|(_, v)| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| {
            ctx.pool_upsert("_fsm", "current", json!(initial));
            initial.clone()
        });

    // Entry actions fire only on transitions, not on construction.
    // The _ suppresses unused variable warning; first_run remains available for guards if needed.
    let _ = first_run;

    // ─── Handle control messages ───
    if let Some(msg) = payload.get("control") {
        let cmd = match msg {
            Message::String(s) => s.to_string(),
            _ => String::new(),
        };
        if cmd == "reset" {
            ctx.pool_upsert("_fsm", "current", json!(initial));
            ctx.pool_upsert("_fsm", "timeout_elapsed", json!(0.0));
            // Reset context
            if let Some(Value::Object(init_ctx)) = config.get("context") {
                for (k, v) in init_ctx {
                    ctx.pool_upsert("_fsm_ctx", k, v.clone());
                }
            }
            let mut out = HashMap::new();
            out.insert("state".to_string(), Message::String(initial.clone().into()));
            return Ok(out);
        } else if let Some(target) = cmd.strip_prefix("set:") {
            ctx.pool_upsert("_fsm", "current", json!(target));
            ctx.pool_upsert("_fsm", "timeout_elapsed", json!(0.0));
            let mut out = HashMap::new();
            out.insert(
                "state".to_string(),
                Message::String(target.to_string().into()),
            );
            return Ok(out);
        }
    }

    // ─── Handle data injection (context update, no transition) ───
    if let Some(Message::Object(obj)) = payload.get("data") {
        let v: Value = obj.as_ref().clone().into();
        if let Some(map) = v.as_object() {
            for (k, val) in map {
                ctx.pool_upsert("_fsm_ctx", k, val.clone());
            }
        }
        return Ok(HashMap::new());
    }

    // ─── Resolve event ───
    let (event_name, event_payload) = resolve_event(&payload);

    // ─── Handle tick (timeout transitions) ───
    let mut is_timeout = false;
    if payload.contains_key("tick") && event_name.is_none() {
        static FSM_TICK_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let tc = FSM_TICK_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if tc % 100 == 0 {
            eprintln!(
                "[fsm:{}] tick={tc} state={current_state}",
                ctx.get_config().get_node_id()
            );
        }
        let state_def = states.get(&current_state);
        if let Some(timeout_trans) = state_def
            .and_then(|s| s.get("on"))
            .and_then(|on| on.get("_timeout"))
        {
            let delay = timeout_trans
                .get("delay")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0);
            let dt = config
                .get("dt")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0 / 30.0);
            let elapsed: f64 = ctx
                .get_pool("_fsm")
                .into_iter()
                .find(|(k, _)| k == "timeout_elapsed")
                .and_then(|(_, v)| v.as_f64())
                .unwrap_or(0.0);
            let new_elapsed = elapsed + dt;
            if new_elapsed >= delay {
                is_timeout = true;
                ctx.pool_upsert("_fsm", "timeout_elapsed", json!(0.0));
            } else {
                ctx.pool_upsert("_fsm", "timeout_elapsed", json!(new_elapsed));
                return Ok(HashMap::new());
            }
        } else {
            return Ok(HashMap::new());
        }
    }

    // ─── Find matching transition ───
    let event = if is_timeout {
        Some("_timeout".to_string())
    } else {
        event_name
    };

    if event.is_none() {
        return Ok(HashMap::new());
    }
    let event = event.unwrap();

    // Build evaluation scope: event payload + context
    let eval_scope = build_eval_scope(&event_payload, &ctx);

    let state_def = match states.get(&current_state) {
        Some(s) => s,
        None => return Ok(HashMap::new()),
    };

    let transition = find_transition(state_def, &event, &guards_def, &eval_scope);

    if let Some(target) = transition {
        execute_transition(
            &ctx,
            &states,
            &guards_def,
            &current_state,
            &target,
            &event,
            &eval_scope,
        )
    } else {
        Ok(HashMap::new())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Event resolution
// ═══════════════════════════════════════════════════════════════════════════

fn resolve_event(payload: &HashMap<String, Message>) -> (Option<String>, Value) {
    if let Some(msg) = payload.get("event") {
        match msg {
            Message::String(s) => (Some(s.to_string()), Value::Null),
            Message::Object(obj) => {
                let v: Value = obj.as_ref().clone().into();
                let name = v
                    .get("type")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string());
                let payload = v.get("payload").cloned().unwrap_or(Value::Null);
                (name, payload)
            }
            _ => (None, Value::Null),
        }
    } else {
        (None, Value::Null)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Guard evaluation
// ═══════════════════════════════════════════════════════════════════════════

fn build_eval_scope(event_payload: &Value, ctx: &ActorContext) -> Value {
    let mut scope = serde_json::Map::new();

    // Merge context
    for (k, v) in ctx.get_pool("_fsm_ctx") {
        scope.insert(k, v);
    }

    // Merge event payload (overwrites context keys)
    if let Some(obj) = event_payload.as_object() {
        for (k, v) in obj {
            scope.insert(k.clone(), v.clone());
        }
    }

    Value::Object(scope)
}

fn evaluate_guard(guard_name: &str, guards_def: &Value, scope: &Value) -> bool {
    let guard = match guards_def.get(guard_name) {
        Some(g) => g,
        None => return true, // unknown guard = pass
    };

    let field = guard.get("field").and_then(|v| v.as_str());
    let operator = guard
        .get("operator")
        .and_then(|v| v.as_str())
        .unwrap_or("is");
    let rule_value = guard.get("value");

    let field_value = if let Some(field_name) = field {
        scope.get(field_name).cloned()
    } else {
        Some(scope.clone())
    };

    let field_value = match field_value {
        Some(v) => v,
        None => return operator == "empty", // missing field = empty
    };

    match operator {
        "is" => rule_value == Some(&field_value),
        "is_not" => rule_value != Some(&field_value),
        "contains" => match (&field_value, rule_value) {
            (Value::String(s), Some(Value::String(needle))) => s.contains(needle.as_str()),
            (Value::Array(arr), Some(val)) => arr.contains(val),
            _ => false,
        },
        "not_contains" => match (&field_value, rule_value) {
            (Value::String(s), Some(Value::String(needle))) => !s.contains(needle.as_str()),
            (Value::Array(arr), Some(val)) => !arr.contains(val),
            _ => true,
        },
        "greater_than" | "gt" => match (&field_value, rule_value) {
            (Value::Number(a), Some(Value::Number(b))) => {
                a.as_f64().unwrap_or(0.0) > b.as_f64().unwrap_or(0.0)
            }
            _ => false,
        },
        "less_than" | "lt" => match (&field_value, rule_value) {
            (Value::Number(a), Some(Value::Number(b))) => {
                a.as_f64().unwrap_or(0.0) < b.as_f64().unwrap_or(0.0)
            }
            _ => false,
        },
        "greater_equal" | "gte" => match (&field_value, rule_value) {
            (Value::Number(a), Some(Value::Number(b))) => {
                a.as_f64().unwrap_or(0.0) >= b.as_f64().unwrap_or(0.0)
            }
            _ => false,
        },
        "less_equal" | "lte" => match (&field_value, rule_value) {
            (Value::Number(a), Some(Value::Number(b))) => {
                a.as_f64().unwrap_or(0.0) <= b.as_f64().unwrap_or(0.0)
            }
            _ => false,
        },
        "empty" => match &field_value {
            Value::Null => true,
            Value::String(s) => s.is_empty(),
            Value::Array(arr) => arr.is_empty(),
            Value::Object(obj) => obj.is_empty(),
            _ => false,
        },
        "not_empty" => match &field_value {
            Value::Null => false,
            Value::String(s) => !s.is_empty(),
            Value::Array(arr) => !arr.is_empty(),
            Value::Object(obj) => !obj.is_empty(),
            _ => true,
        },
        "in" => match rule_value {
            Some(Value::Array(arr)) => arr.contains(&field_value),
            _ => false,
        },
        "not_in" => match rule_value {
            Some(Value::Array(arr)) => !arr.contains(&field_value),
            _ => true,
        },
        _ => false,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Transition resolution
// ═══════════════════════════════════════════════════════════════════════════

fn find_transition(
    state_def: &Value,
    event: &str,
    guards_def: &Value,
    scope: &Value,
) -> Option<String> {
    let on = state_def.get("on")?;
    let trans = on.get(event)?;

    // Transition can be a string (shorthand) or object
    if let Some(target) = trans.as_str() {
        return Some(target.to_string());
    }

    // Array of guarded transitions — first match wins
    if let Some(arr) = trans.as_array() {
        for t in arr {
            let guard = t.get("guard").and_then(|g| g.as_str());
            if let Some(guard_name) = guard {
                if evaluate_guard(guard_name, guards_def, scope) {
                    return t
                        .get("target")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string());
                }
            } else {
                // No guard = default/fallback
                return t
                    .get("target")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string());
            }
        }
        return None;
    }

    // Single object transition with optional guard
    if let Some(guard_name) = trans.get("guard").and_then(|g| g.as_str()) {
        if !evaluate_guard(guard_name, guards_def, scope) {
            return None;
        }
    }

    trans
        .get("target")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
}

/// Find a transient ("always") transition — evaluated on entry, no event needed.
fn find_transient_transition(
    state_def: &Value,
    guards_def: &Value,
    scope: &Value,
) -> Option<String> {
    let always = state_def.get("always")?;

    if let Some(arr) = always.as_array() {
        for t in arr {
            let guard = t.get("guard").and_then(|g| g.as_str());
            if let Some(guard_name) = guard {
                if evaluate_guard(guard_name, guards_def, scope) {
                    return t
                        .get("target")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string());
                }
            } else {
                return t
                    .get("target")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string());
            }
        }
    }

    // Single object shorthand
    if always.is_object() {
        let guard = always.get("guard").and_then(|g| g.as_str());
        if let Some(guard_name) = guard {
            if !evaluate_guard(guard_name, guards_def, scope) {
                return None;
            }
        }
        return always
            .get("target")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());
    }

    None
}

// ═══════════════════════════════════════════════════════════════════════════
// Transition execution
// ═══════════════════════════════════════════════════════════════════════════

fn execute_transition(
    ctx: &ActorContext,
    states: &Value,
    guards_def: &Value,
    from: &str,
    to: &str,
    event: &str,
    scope: &Value,
) -> Result<HashMap<String, Message>, Error> {
    let mut out = HashMap::new();
    let mut emit_values = serde_json::Map::new();
    let mut current = from.to_string();
    let mut target = to.to_string();
    let mut depth = 0;

    loop {
        // Execute exit action of current state
        if let Some(exit_action) = states.get(&current).and_then(|s| s.get("exit")) {
            apply_action(exit_action, ctx, &mut emit_values);
        }

        // Update state
        ctx.pool_upsert("_fsm", "current", json!(target));
        ctx.pool_upsert("_fsm", "timeout_elapsed", json!(0.0));

        // Execute entry action of target state
        if let Some(entry_action) = states.get(&target).and_then(|s| s.get("entry")) {
            apply_action(entry_action, ctx, &mut emit_values);
        }

        // Check for final state
        let is_final = states
            .get(&target)
            .and_then(|s| s.get("type"))
            .and_then(|t| t.as_str())
            == Some("final");

        if is_final {
            out.insert("done".to_string(), Message::Flow);
        }

        // Check for transient transition (always)
        let scope = build_eval_scope(&Value::Null, ctx);
        let transient = states
            .get(&target)
            .and_then(|s| find_transient_transition(s, guards_def, &scope));

        if let Some(next) = transient {
            depth += 1;
            if depth >= MAX_TRANSIENT_DEPTH {
                eprintln!(
                    "[FsmActor] max transient depth ({}) reached at state '{}'",
                    MAX_TRANSIENT_DEPTH, target
                );
                break;
            }
            current = target;
            target = next;
            continue;
        }

        break;
    }

    // ─── Build outputs ───

    // state: final state name
    let final_state = ctx
        .get_pool("_fsm")
        .into_iter()
        .find(|(k, _)| k == "current")
        .and_then(|(_, v)| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| target.clone());

    out.insert(
        "state".to_string(),
        Message::String(final_state.clone().into()),
    );

    // transition: from/to/event
    out.insert(
        "transition".to_string(),
        Message::object(EncodableValue::from(json!({
            "from": from,
            "to": final_state,
            "event": event,
        }))),
    );

    // emit: signal with id = target state, data = merged entry/exit values.
    // Subscribers can filter by state name to react to specific transitions.
    if !emit_values.is_empty() {
        out.insert(
            "emit".to_string(),
            Message::object(EncodableValue::from(json!({
                "id": final_state,
                "data": Value::Object(emit_values.clone()),
            }))),
        );
        // data: flat values for direct wiring to renderer/downstream
        out.insert(
            "data".to_string(),
            Message::object(EncodableValue::from(Value::Object(emit_values))),
        );
    }

    // context: full snapshot if requested by any entry action
    let should_emit_ctx = states
        .get(&final_state)
        .and_then(|s| s.get("entry"))
        .and_then(|a| a.get("emit_context"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if should_emit_ctx {
        let ctx_map: serde_json::Map<String, Value> =
            ctx.get_pool("_fsm_ctx").into_iter().collect();
        out.insert(
            "context".to_string(),
            Message::object(EncodableValue::from(Value::Object(ctx_map))),
        );
    }

    Ok(out)
}

// ═══════════════════════════════════════════════════════════════════════════
// Actions (entry/exit)
// ═══════════════════════════════════════════════════════════════════════════

fn apply_action(
    action: &Value,
    ctx: &ActorContext,
    emit_values: &mut serde_json::Map<String, Value>,
) {
    // emit: key-value pairs to output
    if let Some(obj) = action.get("emit").and_then(|v| v.as_object()) {
        for (k, v) in obj {
            emit_values.insert(k.clone(), v.clone());
        }
    }

    // assign: context mutations
    if let Some(obj) = action.get("assign").and_then(|v| v.as_object()) {
        for (field, mutation) in obj {
            let current: f64 = ctx
                .get_pool("_fsm_ctx")
                .into_iter()
                .find(|(k, _)| k == field)
                .and_then(|(_, v)| v.as_f64())
                .unwrap_or(0.0);

            if let Some(op_obj) = mutation.as_object() {
                let op = op_obj.get("op").and_then(|v| v.as_str()).unwrap_or("set");
                let val = op_obj.get("value").and_then(|v| v.as_f64()).unwrap_or(1.0);

                let new_val = match op {
                    "set" => val,
                    "increment" => current + val,
                    "decrement" => current - val,
                    "add" => current + val,
                    "multiply" => current * val,
                    "toggle" => {
                        if current == 0.0 {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    _ => current,
                };
                ctx.pool_upsert("_fsm_ctx", field, json!(new_val));
            } else {
                // Direct value assignment
                ctx.pool_upsert("_fsm_ctx", field, mutation.clone());
            }
        }
    }
}
