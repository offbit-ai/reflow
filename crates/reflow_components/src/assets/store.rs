//! Asset store actor — writes data into the AssetDB.
//!
//! Accepts binary or JSON data and stores it with a name, type, and tags.
//! Returns the asset ID for downstream reference.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
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
        .get("dbPath")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");
    let asset_type = config
        .get("assetType")
        .and_then(|v| v.as_str())
        .unwrap_or("generic");
    let name = config
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed");
    let asset_metadata = config.get("metadata").cloned().unwrap_or(json!({}));
    let tags: Vec<&str> = config
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let db = get_or_create_db(db_path)?;

    // Store binary data or JSON data
    let id = if let Some(Message::Bytes(bytes)) = payload.get("data") {
        db.store(asset_type, name, bytes, asset_metadata, &tags)?
    } else if let Some(Message::Object(obj)) = payload.get("json_data") {
        let v: Value = obj.as_ref().clone().into();
        db.store_json(asset_type, name, v, asset_metadata, &tags)?
    } else if let Some(Message::String(s)) = payload.get("data") {
        db.store(asset_type, name, s.as_bytes(), asset_metadata, &tags)?
    } else {
        return Ok(error_output("Expected Bytes on data or Object on json_data"));
    };

    let mut out = HashMap::new();
    out.insert("asset_id".to_string(), Message::String(id.clone().into()));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "id": id,
            "name": name,
            "assetType": asset_type,
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
