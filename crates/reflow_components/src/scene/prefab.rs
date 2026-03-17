//! Entity actor — the primary node for creating entities in the DAG.
//!
//! One node = one entity. The entity name is in config (`$name`).
//! Components come from config sections and inports.
//!
//! ## How it works
//!
//! Config components are written on first invocation — the entity
//! is immediately valid with all config-defined sections.
//!
//! Inport data (mesh, material, component) pools and merges on top
//! of what's already in AssetDB. Since writes are upserts, repeated
//! invocations are idempotent — no duplicates, no partial state.
//!
//! For spawn mode, send entity names to the `spawn` inport after
//! the template entity is fully defined.
//!
//! ## Config = component sections
//!
//! ```json
//! {
//!   "$name": "enemy_01",
//!   "$db": "./game.db",
//!   "transform": { "position": [5, 0, 3] },
//!   "rigidbody": { "bodyType": "dynamic", "mass": 60 },
//!   "collider": { "shape": "capsule", "radius": 0.3 },
//!   "material": { "albedo": [0.3, 0.6, 0.2] },
//!   "behavior": { "rules": [...] },
//!   "state_machine": { "current": "idle", ... }
//! }
//! ```
//!
//! ## Inports
//!
//! - `mesh` — binary mesh data (pooled)
//! - `material` — overrides config material (pooled)
//! - `transform` — overrides config transform (pooled)
//! - `component` — generic `{ "name": "...", "data": {...} }` (pooled, multiple connections)
//! - `spawn` — instantiate: send entity name, get a copy of this entity
//!
//! ## Spawn mode
//!
//! ```text
//! "crate_42" → spawn → Entity($name: "crate_tpl") → entity_id
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
    inports::<100>(mesh, material, transform, component, spawn),
    outports::<10>(entity_id, prefab, metadata),
    state(MemoryState)
)]
pub async fn prefab_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let entity_name = config
        .get("$name")
        .or_else(|| config.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("entity")
        .to_string();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");
    let stride = config
        .get("stride")
        .and_then(|v| v.as_u64())
        .unwrap_or(24) as usize;

    let db = get_or_create_db(db_path)?;

    // ─── Spawn mode ───
    if let Some(Message::String(new_entity)) = payload.get("spawn") {
        let new_name = new_entity.to_string();
        db.spawn_from(&entity_name, &new_name)?;

        let mut out = HashMap::new();
        out.insert("entity_id".to_string(), Message::String(new_name.clone().into()));
        out.insert(
            "metadata".to_string(),
            Message::object(EncodableValue::from(json!({
                "action": "spawn",
                "template": entity_name,
                "entity": new_name,
                "components": db.components_of(&new_name).unwrap_or_default(),
            }))),
        );
        return Ok(out);
    }

    // ─── Pool inport data ───

    if let Some(Message::Bytes(b)) = payload.get("mesh") {
        let encoded = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(&**b)
        };
        ctx.pool_upsert("_data", "mesh_b64", json!(encoded));
    }
    if let Some(Message::Object(obj)) = payload.get("material") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_data", "material", v);
    }
    if let Some(Message::Object(obj)) = payload.get("transform") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_data", "transform", v);
    }
    if let Some(Message::Object(obj)) = payload.get("component") {
        let v: Value = obj.as_ref().clone().into();
        let comp_name = v.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !comp_name.is_empty() {
            ctx.pool_upsert("_components", &comp_name, v);
        }
    }

    // ─── Write entity: config + pooled data ───

    // Config components
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
    let pool_data: Vec<(String, Value)> = ctx.get_pool("_data").into_iter().collect();
    for (key, val) in &pool_data {
        match key.as_str() {
            "mesh_b64" => {
                if let Some(encoded) = val.as_str() {
                    use base64::Engine;
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) {
                        db.set_component(&entity_name, "mesh", &bytes, json!({"stride": stride}))?;
                    }
                }
            }
            "material" => {
                db.merge_component_json(&entity_name, "material", val.clone(), json!({}))?;
            }
            "transform" => {
                db.merge_component_json(&entity_name, "transform", val.clone(), json!({}))?;
            }
            _ => {}
        }
    }

    // Pooled generic components
    let pooled_comps: Vec<(String, Value)> = ctx.get_pool("_components").into_iter().collect();
    for (_, comp) in &pooled_comps {
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
