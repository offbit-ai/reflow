//! Entity/Prefab actor — creates entities in AssetDB or passes data through.
//!
//! Two modes:
//! - **With `$db`**: Writes components to AssetDB. Full entity node.
//! - **Without `$db`**: Legacy passthrough — wraps mesh/material as prefab descriptor.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
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

    let entity_name = match payload.get("entity") {
        Some(Message::String(s)) => s.to_string(),
        _ => config
            .get("$name")
            .or_else(|| config.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("prefab")
            .to_string(),
    };

    let db_path = config.get("$db").and_then(|v| v.as_str());
    let stride = config.get("stride").and_then(|v| v.as_u64()).unwrap_or(24) as usize;

    // ─── AssetDB mode ($db configured) ───
    if let Some(path) = db_path {
        let db = get_or_create_db(path)?;

        // Spawn mode
        if let Some(Message::String(new_entity)) = payload.get("spawn") {
            let new_name = new_entity.to_string();
            db.spawn_from(&entity_name, &new_name)?;
            let mut out = HashMap::new();
            out.insert(
                "entity_id".to_string(),
                Message::String(new_name.clone().into()),
            );
            out.insert(
                "metadata".to_string(),
                Message::object(EncodableValue::from(json!({
                    "action": "spawn", "template": entity_name, "entity": new_name,
                }))),
            );
            return Ok(out);
        }

        // Pool inport data
        if let Some(Message::Bytes(b)) = payload.get("mesh") {
            let encoded = {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.encode(&**b)
            };
            ctx.pool_upsert("_data", "mesh_b64", json!(encoded));
        }
        if let Some(Message::Object(obj)) = payload.get("material") {
            ctx.pool_upsert("_data", "material", obj.as_ref().clone().into());
        }
        if let Some(Message::Object(obj)) = payload.get("transform") {
            ctx.pool_upsert("_data", "transform", obj.as_ref().clone().into());
        }
        if let Some(Message::Object(obj)) = payload.get("component") {
            let v: Value = obj.as_ref().clone().into();
            let comp_name = v
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !comp_name.is_empty() {
                ctx.pool_upsert("_components", &comp_name, v);
            }
        }

        // Write config components
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

        // Write pooled data
        for (key, val) in ctx.get_pool("_data").into_iter() {
            match key.as_str() {
                "mesh_b64" => {
                    if let Some(encoded) = val.as_str() {
                        use base64::Engine;
                        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded)
                        {
                            db.set_component(
                                &entity_name,
                                "mesh",
                                &bytes,
                                json!({"stride": stride}),
                            )?;
                        }
                    }
                }
                "material" => {
                    db.merge_component_json(&entity_name, "material", val, json!({}))?;
                }
                "transform" => {
                    db.merge_component_json(&entity_name, "transform", val, json!({}))?;
                }
                _ => {}
            }
        }

        for (_, comp) in ctx.get_pool("_components").into_iter() {
            if let (Some(cn), Some(cd)) =
                (comp.get("name").and_then(|v| v.as_str()), comp.get("data"))
            {
                db.merge_component_json(&entity_name, cn, cd.clone(), json!({}))?;
            }
        }

        let components = db.components_of(&entity_name).unwrap_or_default();
        let mut out = HashMap::new();
        out.insert(
            "entity_id".to_string(),
            Message::String(entity_name.clone().into()),
        );
        out.insert(
            "prefab".to_string(),
            Message::object(EncodableValue::from(
                json!({"id": entity_name, "components": components}),
            )),
        );
        out.insert(
            "metadata".to_string(),
            Message::object(EncodableValue::from(
                json!({"action": "define", "entity": entity_name, "components": components}),
            )),
        );
        return Ok(out);
    }

    // ─── Legacy passthrough mode (no $db — original behavior) ───
    let mesh_size = match payload.get("mesh") {
        Some(Message::Bytes(b)) => b.len(),
        _ => 0,
    };

    let material = match payload.get("material") {
        Some(Message::Object(obj)) => obj.as_ref().clone().into(),
        _ => config
            .get("material")
            .cloned()
            .unwrap_or(json!({"color": [0.8, 0.8, 0.8]})),
    };

    let prefab = json!({
        "id": entity_name,
        "type": "prefab",
        "meshSize": mesh_size,
        "material": material,
        "stride": stride,
    });

    let mut out = HashMap::new();
    out.insert(
        "prefab".to_string(),
        Message::object(EncodableValue::from(prefab)),
    );
    if let Some(mesh) = payload.get("mesh") {
        out.insert("prefab_mesh".to_string(), mesh.clone());
    }
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(
            json!({"name": entity_name, "meshSize": mesh_size}),
        )),
    );
    Ok(out)
}
