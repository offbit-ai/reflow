//! Asset load actor — retrieves a single entity from the AssetDB.
//!
//! ECS-style direct access by entity ID:
//!
//!   config: { "id": "snake:mesh" }
//!   config: { "id": "character/idle:animation" }
//!
//! The ID can also come from the `asset_id` inport for dynamic lookups.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    AssetLoadActor,
    inports::<10>(asset_id),
    outports::<1>(data, json_data, metadata, error),
    state(MemoryState)
)]
pub async fn asset_load_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .or_else(|| config.get("dbPath"))
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");

    // Entity ID from config or inport
    let id = config
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| match payload.get("asset_id") {
            Some(Message::String(s)) => Some(s.to_string()),
            _ => None,
        });

    let id = match id {
        Some(id) => id,
        None => {
            return Ok(error_output(
                "Provide entity ID via config.id or asset_id inport",
            ))
        }
    };

    let db = get_or_create_db(db_path)?;

    let asset = match db.get(&id) {
        Ok(a) => a,
        Err(e) => return Ok(error_output(&format!("{}", e))),
    };

    let mut out = HashMap::new();

    // Output as JSON if inline, always output raw bytes
    if asset.entry.inline_data.is_some() {
        let v: Value = serde_json::from_slice(&asset.data).unwrap_or(Value::Null);
        out.insert(
            "json_data".to_string(),
            Message::object(EncodableValue::from(v)),
        );
    }
    out.insert("data".to_string(), Message::bytes(asset.data));

    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "id": asset.entry.id,
            "name": asset.entry.name,
            "type": asset.entry.asset_type,
            "size": asset.entry.blob_size,
            "tags": asset.entry.tags,
            "metadata": asset.entry.metadata,
        }))),
    );
    Ok(out)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
