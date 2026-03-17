//! Asset load actor — reads data from the AssetDB.
//!
//! Loads by asset ID, name, or name+type. Outputs the binary/JSON data
//! and the asset metadata.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    AssetLoadActor,
    inports::<10>(asset_id, name),
    outports::<1>(data, json_data, metadata, error),
    state(MemoryState)
)]
pub async fn asset_load_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("dbPath")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");
    let asset_type = config.get("assetType").and_then(|v| v.as_str());

    // ID from config or inport
    let id = config
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| match payload.get("asset_id") {
            Some(Message::String(s)) => Some(s.to_string()),
            _ => None,
        });

    // Name from config or inport
    let name = config
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| match payload.get("name") {
            Some(Message::String(s)) => Some(s.to_string()),
            _ => None,
        });

    let db = get_or_create_db(db_path)?;

    let asset = if let Some(id) = id {
        db.load(&id)?
    } else if let Some(name) = name {
        if let Some(at) = asset_type {
            db.load_by_name_and_type(&name, at)?
        } else {
            db.load_by_name(&name)?
        }
    } else {
        return Ok(error_output("Provide asset_id or name to load"));
    };

    let mut out = HashMap::new();

    // Output as bytes or JSON depending on whether inline_data exists
    if asset.entry.inline_data.is_some() {
        let v: Value = serde_json::from_slice(&asset.data).unwrap_or(Value::Null);
        out.insert(
            "json_data".to_string(),
            Message::object(EncodableValue::from(v)),
        );
    }
    // Always output raw bytes too
    out.insert("data".to_string(), Message::bytes(asset.data));

    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "id": asset.entry.id,
            "name": asset.entry.name,
            "assetType": asset.entry.asset_type,
            "blobSize": asset.entry.blob_size,
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
