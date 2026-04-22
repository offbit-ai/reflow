//! Asset store actor — puts an entity into the AssetDB.
//!
//! ECS-style: the entity ID is the key, put is an upsert.
//!
//!   config: { "id": "snake:mesh" }
//!   config: { "id": "gold:material", "tags": ["pbr", "metallic"] }
//!
//! ID convention: "name:type" — the type is inferred from the suffix.
//! "snake:mesh", "character/idle:animation", "gold:material"

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    AssetStoreActor,
    inports::<10>(data, json_data),
    outports::<1>(asset_id, metadata, error),
    state(MemoryState)
)]
pub async fn asset_store_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .or_else(|| config.get("dbPath"))
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");

    let id = config
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("AssetStoreActor requires config.id"))?;

    let asset_metadata = config.get("metadata").cloned().unwrap_or(json!({}));
    let tags: Vec<&str> = config
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let db = get_or_create_db(db_path)?;

    // Put binary data or JSON data
    if let Some(Message::Bytes(bytes)) = payload.get("data") {
        db.put(id, bytes, asset_metadata)?;
    } else if let Some(Message::Object(obj)) = payload.get("json_data") {
        let v: Value = obj.as_ref().clone().into();
        db.put_json(id, v, asset_metadata)?;
    } else if let Some(Message::String(s)) = payload.get("data") {
        db.put(id, s.as_bytes(), asset_metadata)?;
    } else {
        return Ok(error_output(
            "Expected Bytes on data or Object on json_data",
        ));
    }

    // Apply tags
    if !tags.is_empty() {
        db.tag(id, &tags)?;
    }

    let mut out = HashMap::new();
    out.insert(
        "asset_id".to_string(),
        Message::String(id.to_string().into()),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "id": id,
            "dbPath": db_path,
        }))),
    );
    Ok(out)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
