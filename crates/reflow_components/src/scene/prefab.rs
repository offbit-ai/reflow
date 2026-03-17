//! Prefab actor — entity template and spawner via AssetDB.
//!
//! Two modes:
//!
//! ## 1. Define a prefab (store template in AssetDB)
//!
//! Wire mesh, material, and other data into the prefab. It stores them
//! as components on a template entity in the AssetDB.
//!
//! ```text
//! TubeMesh → mesh ────┐
//! NoiseGen → material ─┼→ PrefabActor(name: "crate", $db: "./game.db")
//!                      │     stores: crate_tpl:mesh, crate_tpl:material, crate_tpl:transform
//! ```
//!
//! ## 2. Spawn from a prefab (instantiate)
//!
//! Send an entity name to the `spawn` inport. The actor calls
//! `db.spawn_from(template, new_entity)` and outputs the new entity ID.
//!
//! ```text
//! "crate_42" → spawn → PrefabActor(template: "crate_tpl") → entity_id
//! ```
//!
//! ## Config
//!
//! Any key that isn't a control key (`name`, `template`, `$db`, `stride`)
//! is stored as a component on the template entity. This means prefabs
//! can include physics, behaviors, state machines — the full entity:
//!
//! ```json
//! {
//!   "name": "enemy",
//!   "$db": "./game.db",
//!   "transform": { "position": [0, 0, 0] },
//!   "material": { "albedo": [0.3, 0.6, 0.2] },
//!   "rigidbody": { "bodyType": "dynamic", "mass": 60 },
//!   "collider": { "shape": "capsule", "radius": 0.3, "height": 1.6 },
//!   "behavior": {
//!     "rules": [
//!       { "name": "patrol", "target": "transform.position.x", "expr": "sin(time) * 5" }
//!     ]
//!   },
//!   "state_machine": {
//!     "current": "idle",
//!     "states": { "idle": {}, "chase": {}, "attack": {} },
//!     "transitions": [
//!       { "from": "idle", "to": "chase", "trigger": "playerNear" }
//!     ]
//!   }
//! }
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    PrefabActor,
    inports::<10>(mesh, material, transform, component, spawn),
    outports::<10>(entity_id, prefab, metadata),
    state(MemoryState)
)]
pub async fn prefab_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let name = config
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("prefab")
        .to_string();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");

    let default_template = format!("{}_tpl", name);
    let template_name = config
        .get("template")
        .and_then(|v| v.as_str())
        .unwrap_or(&default_template);

    let stride = config
        .get("stride")
        .and_then(|v| v.as_u64())
        .unwrap_or(24) as usize;

    let db = get_or_create_db(db_path)?;

    // ─── Mode 1: Define prefab (store components on template entity) ───

    // Mesh from inport
    if let Some(Message::Bytes(mesh_bytes)) = payload.get("mesh") {
        db.set_component(
            template_name,
            "mesh",
            mesh_bytes,
            json!({"stride": stride}),
        )?;
    }

    // Material from inport or config
    let material = match payload.get("material") {
        Some(Message::Object(obj)) => {
            let v: Value = obj.as_ref().clone().into();
            Some(v)
        }
        _ => config.get("material").cloned(),
    };
    if let Some(mat) = material {
        db.set_component_json(template_name, "material", mat, json!({}))?;
    }

    // Transform from inport or config
    let transform = match payload.get("transform") {
        Some(Message::Object(obj)) => {
            let v: Value = obj.as_ref().clone().into();
            Some(v)
        }
        _ => config.get("transform").cloned(),
    };
    if let Some(tf) = transform {
        db.set_component_json(template_name, "transform", tf, json!({}))?;
    }

    // Generic component inport: { "name": "rigidbody", "data": {...} }
    // Wire any actor's output into a prefab as a named component.
    if let Some(Message::Object(obj)) = payload.get("component") {
        let v: Value = obj.as_ref().clone().into();
        if let Some(comp_name) = v.get("name").and_then(|v| v.as_str()) {
            if let Some(comp_data) = v.get("data") {
                if let Value::Object(_) | Value::Array(_) = comp_data {
                    db.set_component_json(template_name, comp_name, comp_data.clone(), json!({}))?;
                } else if let Some(s) = comp_data.as_str() {
                    // String data → store as binary
                    db.set_component(template_name, comp_name, s.as_bytes(), json!({}))?;
                }
            }
        }
    }

    // Store any other config keys as components (excluding control keys)
    for (key, val) in &config {
        match key.as_str() {
            "name" | "template" | "$db" | "stride" | "material" | "transform" => continue,
            component => {
                if val.is_object() || val.is_array() {
                    db.set_component_json(template_name, component, val.clone(), json!({}))?;
                }
            }
        }
    }

    let mut out = HashMap::new();

    // ─── Mode 2: Spawn instance ───

    if let Some(Message::String(new_entity)) = payload.get("spawn") {
        let new_name = new_entity.to_string();
        db.spawn_from(template_name, &new_name)?;

        out.insert(
            "entity_id".to_string(),
            Message::String(new_name.clone().into()),
        );

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

    // ─── Output prefab descriptor (backward compatible) ───

    let components = db.components_of(template_name).unwrap_or_default();

    let prefab_desc = json!({
        "id": template_name,
        "name": name,
        "type": "prefab",
        "components": components,
        "stride": stride,
    });

    out.insert(
        "prefab".to_string(),
        Message::object(EncodableValue::from(prefab_desc)),
    );

    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "action": "define",
            "template": template_name,
            "components": components,
        }))),
    );

    Ok(out)
}
