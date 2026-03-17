//! Entity actor — the primary node for creating entities in the DAG.
//!
//! One node = one entity. All components defined in config. Toggle
//! sections on/off in the visual editor. Only enabled components
//! are written to AssetDB.
//!
//! ## Accumulate then flush
//!
//! Inport data is pooled as it arrives. The entity is only written
//! to AssetDB when all connected inports have fired. This prevents
//! partial entities from being visible to systems mid-tick.
//!
//! ## Standard components (built-in sections)
//!
//! ```json
//! {
//!   "$name": "enemy_01",
//!   "$db": "./game.db",
//!
//!   "transform": { "position": [5, 0, 3], "rotation": [0, 0, 0, 1] },
//!   "rigidbody": { "bodyType": "dynamic", "mass": 60 },
//!   "collider": { "shape": "capsule", "radius": 0.3, "height": 1.8 },
//!   "material": { "albedo": [0.3, 0.6, 0.2], "roughness": 0.7 },
//!   "behavior": { "rules": [...] },
//!   "state_machine": { "current": "idle", ... },
//!   "billboard": { "mode": "screen", "text": "Enemy" },
//!   "light": { "type": "point", "color": [1, 0.6, 0.2] },
//!   "camera": { "mode": "thirdPerson", "target": "player" },
//!   "text": { "content": "Hello", "font": "roboto:font" },
//!   "tween": { "target": "transform.position", ... },
//!   "skybox": { "mode": "gradient" },
//!   "weather": { "type": "rain", "intensity": 0.7 },
//!   "bind": true
//! }
//! ```
//!
//! ## Inports
//!
//! - `entity` — entity name (overrides `$name` config)
//! - `mesh` — binary mesh data from upstream
//! - `material` — material from upstream actor
//! - `transform` — transform from upstream actor
//! - `component` — generic `{ "name": "...", "data": {...} }` (pooled, multiple connections)
//! - `spawn` — send an entity name to instantiate from this as a template
//!
//! ## Spawn mode
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

const CONTROL_KEYS: &[&str] = &["$name", "$db", "$template", "name", "template", "stride"];

#[actor(
    PrefabActor,
    inports::<100>(entity, mesh, material, transform, component, spawn),
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

    // ─── Pool all incoming data (don't write yet) ───

    if let Some(Message::String(s)) = payload.get("entity") {
        ctx.pool_upsert("_entity", "name", json!(s.to_string()));
    }
    if let Some(Message::Bytes(b)) = payload.get("mesh") {
        // Store mesh bytes as base64 in pool (temporary)
        let encoded = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(&**b)
        };
        ctx.pool_upsert("_entity", "mesh_b64", json!(encoded));
    }
    if let Some(Message::Object(obj)) = payload.get("material") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_entity", "material", v);
    }
    if let Some(Message::Object(obj)) = payload.get("transform") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_entity", "transform", v);
    }
    if let Some(Message::Object(obj)) = payload.get("component") {
        let v: Value = obj.as_ref().clone().into();
        let comp_name = v.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !comp_name.is_empty() {
            ctx.pool_upsert("_components", &comp_name, v);
        }
    }

    // ─── Spawn mode: instantiate from template ───
    if let Some(Message::String(new_entity)) = payload.get("spawn") {
        let template_name = config
            .get("$template")
            .or_else(|| config.get("template"))
            .and_then(|v| v.as_str())
            .or_else(|| config.get("$name").or_else(|| config.get("name")).and_then(|v| v.as_str()))
            .unwrap_or("entity");

        let new_name = new_entity.to_string();
        db.spawn_from(template_name, &new_name)?;

        let mut out = HashMap::new();
        out.insert("entity_id".to_string(), Message::String(new_name.clone().into()));
        out.insert(
            "metadata".to_string(),
            Message::object(EncodableValue::from(json!({
                "action": "spawn",
                "template": template_name,
                "entity": new_name,
                "components": db.components_of(&new_name).unwrap_or_default(),
            }))),
        );
        return Ok(out);
    }

    // ─── Resolve entity name ───
    let entity_name = ctx.get_pool("_entity")
        .into_iter()
        .find(|(k, _)| k == "name")
        .and_then(|(_, v)| v.as_str().map(|s| s.to_string()))
        .or_else(|| {
            config.get("$name")
                .or_else(|| config.get("name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

    let entity_name = match entity_name {
        Some(name) => name,
        None => return Ok(HashMap::new()), // No entity name yet — wait for more data
    };

    // ─── Flush: write all accumulated data to AssetDB ───

    // Config components (static)
    for (key, val) in &config {
        if CONTROL_KEYS.contains(&key.as_str()) || key.starts_with('$') {
            continue;
        }
        match val {
            Value::Object(_) | Value::Array(_) => {
                db.set_component_json(&entity_name, key, val.clone(), json!({}))?;
            }
            Value::Bool(b) => {
                db.set_component_json(&entity_name, key, json!(b), json!({}))?;
            }
            _ => {}
        }
    }

    // Pooled mesh
    if let Some((_, mesh_b64)) = ctx.get_pool("_entity").into_iter().find(|(k, _)| k == "mesh_b64") {
        if let Some(encoded) = mesh_b64.as_str() {
            use base64::Engine;
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) {
                db.set_component(&entity_name, "mesh", &bytes, json!({"stride": stride}))?;
            }
        }
    }

    // Pooled material (overrides config)
    if let Some((_, mat)) = ctx.get_pool("_entity").into_iter().find(|(k, _)| k == "material") {
        db.set_component_json(&entity_name, "material", mat, json!({}))?;
    }

    // Pooled transform (overrides config)
    if let Some((_, tf)) = ctx.get_pool("_entity").into_iter().find(|(k, _)| k == "transform") {
        db.set_component_json(&entity_name, "transform", tf, json!({}))?;
    }

    // Pooled generic components (merged)
    let pooled: Vec<(String, Value)> = ctx.get_pool("_components").into_iter().collect();
    for (_, comp) in &pooled {
        if let (Some(comp_name), Some(comp_data)) = (
            comp.get("name").and_then(|v| v.as_str()),
            comp.get("data"),
        ) {
            db.merge_component_json(&entity_name, comp_name, comp_data.clone(), json!({}))?;
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
