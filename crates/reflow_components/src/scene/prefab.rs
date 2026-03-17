//! Entity actor — the primary node for creating entities in the DAG.
//!
//! One node = one entity. All components defined in config. Toggle
//! sections on/off in the visual editor. Only enabled components
//! are written to AssetDB.
//!
//! ## Standard components (built-in sections)
//!
//! ```json
//! {
//!   "$name": "enemy_01",
//!   "$db": "./game.db",
//!
//!   "transform": { "position": [5, 0, 3], "rotation": [0, 0, 0, 1], "scale": [1, 1, 1] },
//!   "rigidbody": { "bodyType": "dynamic", "mass": 60, "gravityScale": 1.0 },
//!   "collider": { "shape": "capsule", "radius": 0.3, "height": 1.8 },
//!   "material": { "albedo": [0.3, 0.6, 0.2], "roughness": 0.7 },
//!   "billboard": { "mode": "screen", "text": "Enemy", "offset": [0, 2, 0] },
//!   "behavior": { "rules": [{ "name": "patrol", "target": "transform.position.x", "expr": "sin(time) * 5" }] },
//!   "state_machine": { "current": "idle", "states": {...}, "transitions": [...] },
//!   "light": { "type": "point", "color": [1, 0.6, 0.2], "range": 10 },
//!   "camera": { "mode": "thirdPerson", "target": "player", "fov": 60 },
//!   "text": { "content": "Hello", "font": "roboto:font", "fontSize": 24 },
//!   "tween": { "target": "transform.position", "from": [0,0,0], "to": [5,0,0], "duration": 1.0 },
//!   "skybox": { "mode": "gradient", "topColor": [0.1, 0.15, 0.4] },
//!   "weather": { "type": "rain", "intensity": 0.7 },
//!   "bind": true
//! }
//! ```
//!
//! Any key that isn't a `$` control key is a component. The visual editor
//! renders each as a collapsible section with typed property fields.
//!
//! ## Inports
//!
//! - `entity` — entity name (overrides `$name` config)
//! - `mesh` — binary mesh data from upstream
//! - `material` — material from upstream actor
//! - `transform` — transform from upstream actor
//! - `component` — generic `{ "name": "...", "data": {...} }` from ComponentNode
//! - `spawn` — send an entity name to instantiate from this as a template
//!
//! ## Spawn mode (prefab instantiation)
//!
//! ```text
//! "crate_42" → spawn → Entity($template: "crate_tpl") → entity_id
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Control keys that are not components.
const CONTROL_KEYS: &[&str] = &["$name", "$db", "$template", "name", "template", "stride"];

#[actor(
    PrefabActor,
    inports::<10>(entity, mesh, material, transform, component, spawn),
    outports::<10>(entity_id, prefab, metadata),
    state(MemoryState)
)]
pub async fn prefab_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");

    let stride = config
        .get("stride")
        .and_then(|v| v.as_u64())
        .unwrap_or(24) as usize;

    let db = get_or_create_db(db_path)?;

    // Entity name: inport overrides config
    let entity_name = match payload.get("entity") {
        Some(Message::String(s)) => s.to_string(),
        _ => config
            .get("$name")
            .or_else(|| config.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("entity")
            .to_string(),
    };

    let template_name = config
        .get("$template")
        .or_else(|| config.get("template"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // ─── Spawn mode: instantiate from template ───
    if let Some(Message::String(new_entity)) = payload.get("spawn") {
        let tpl = template_name.as_deref().unwrap_or(&entity_name);
        let new_name = new_entity.to_string();
        db.spawn_from(tpl, &new_name)?;

        let mut out = HashMap::new();
        out.insert("entity_id".to_string(), Message::String(new_name.clone().into()));
        out.insert(
            "metadata".to_string(),
            Message::object(EncodableValue::from(json!({
                "action": "spawn",
                "template": tpl,
                "entity": new_name,
                "components": db.components_of(&new_name).unwrap_or_default(),
            }))),
        );
        return Ok(out);
    }

    // ─── Define mode: store components on entity ───

    // All config keys that aren't control keys → stored as components
    for (key, val) in &config {
        if CONTROL_KEYS.contains(&key.as_str()) || key.starts_with('$') {
            continue;
        }
        match val {
            Value::Object(_) | Value::Array(_) => {
                db.set_component_json(&entity_name, key, val.clone(), json!({}))?;
            }
            Value::Bool(b) => {
                // Boolean components (e.g. "bind": true)
                db.set_component_json(&entity_name, key, json!(b), json!({}))?;
            }
            _ => {} // Skip scalar config values that aren't components
        }
    }

    // Mesh from inport (binary)
    if let Some(Message::Bytes(mesh_bytes)) = payload.get("mesh") {
        db.set_component(&entity_name, "mesh", mesh_bytes, json!({"stride": stride}))?;
    }

    // Material from inport (overrides config)
    if let Some(Message::Object(obj)) = payload.get("material") {
        let v: Value = obj.as_ref().clone().into();
        db.set_component_json(&entity_name, "material", v, json!({}))?;
    }

    // Transform from inport (overrides config)
    if let Some(Message::Object(obj)) = payload.get("transform") {
        let v: Value = obj.as_ref().clone().into();
        db.set_component_json(&entity_name, "transform", v, json!({}))?;
    }

    // Generic component from inport: { "name": "...", "data": {...} }
    if let Some(Message::Object(obj)) = payload.get("component") {
        let v: Value = obj.as_ref().clone().into();
        if let Some(comp_name) = v.get("name").and_then(|v| v.as_str()) {
            if let Some(comp_data) = v.get("data") {
                db.set_component_json(&entity_name, comp_name, comp_data.clone(), json!({}))?;
            }
        }
    }

    // ─── Output ───
    let components = db.components_of(&entity_name).unwrap_or_default();

    let mut out = HashMap::new();
    out.insert("entity_id".to_string(), Message::String(entity_name.clone().into()));
    out.insert(
        "prefab".to_string(),
        Message::object(EncodableValue::from(json!({
            "id": entity_name,
            "components": components,
        }))),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "action": "define",
            "entity": entity_name,
            "components": components,
        }))),
    );
    Ok(out)
}
