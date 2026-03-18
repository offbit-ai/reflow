//! Component node — defines and attaches a component to an entity.
//!
//! The config IS the component data. `$name` names the component type.
//! This is how you build entities in the DAG.
//!
//! ## Static (config-only, most common)
//!
//! ```text
//! "player" → entity → Component($name: "rigidbody", bodyType: "dynamic", mass: 80)
//! ```
//!
//! The node config defines the rigidbody: `{ "$name": "rigidbody", "bodyType": "dynamic", "mass": 80 }`.
//! When entity arrives, it writes `player:rigidbody` to AssetDB.
//!
//! ## Dynamic fields (DAG-computed overrides)
//!
//! The `data` inport accepts a partial object that merges into the config.
//! Use when a specific field needs to come from upstream computation:
//!
//! ```text
//! MathExpr("health * 0.5") → data ─┐  { "mass": 40.0 }
//!                                    ├→ Component($name: "rigidbody", bodyType: "dynamic", mass: 80)
//! "player" ──────────→ entity ──────┘
//! ```
//!
//! The `data` inport sends `{ "mass": 40.0 }` — only the mass field is
//! overridden. The rest (bodyType, etc.) stays from config.
//!
//! ## Building entities in the DAG
//!
//! ```text
//! "enemy_01" → entity → Component($name: "transform", position: [5, 0, 3])
//! "enemy_01" → entity → Component($name: "rigidbody", bodyType: "dynamic", mass: 60)
//! "enemy_01" → entity → Component($name: "collider", shape: "capsule", radius: 0.3)
//! "enemy_01" → entity → Component($name: "material", albedo: [0.3, 0.6, 0.2])
//! ```
//!
//! Each Component node writes one component. Wire the same entity
//! into multiple Component nodes to build a complete entity.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    ComponentNodeActor,
    inports::<10>(entity, data),
    outports::<1>(output, metadata),
    state(MemoryState)
)]
pub async fn component_node_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let component_name = config
        .get("$name")
        .and_then(|v| v.as_str())
        .unwrap_or("generic");

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");

    // Component data = config fields (excluding $ control keys)
    let mut component_data = serde_json::Map::new();
    for (key, val) in &config {
        if !key.starts_with('$') {
            component_data.insert(key.clone(), val.clone());
        }
    }

    // data inport: partial merge — only overrides fields that are present
    if let Some(Message::Object(obj)) = payload.get("data") {
        let v: Value = obj.as_ref().clone().into();
        if let Value::Object(map) = v {
            for (k, v) in map {
                component_data.insert(k, v);
            }
        }
    }

    let data_value = Value::Object(component_data);

    // Entity from inport (dynamic) or config (static)
    let entity = match payload.get("entity") {
        Some(Message::String(s)) => Some(s.to_string()),
        _ => config
            .get("entity")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    };

    // Write to AssetDB when entity is known
    if let Some(ref entity_name) = entity {
        let db = get_or_create_db(db_path)?;
        db.set_component_json(entity_name, component_name, data_value.clone(), json!({}))?;
    }

    // Output for chaining (e.g. into prefab component inport)
    let mut out = HashMap::new();
    out.insert(
        "output".to_string(),
        Message::object(EncodableValue::from(json!({
            "name": component_name,
            "data": data_value,
        }))),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "component": component_name,
            "entity": entity,
        }))),
    );
    Ok(out)
}
