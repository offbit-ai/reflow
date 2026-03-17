//! Asset query actor — searches the AssetDB by type, name, and tags.
//!
//! Returns matching asset metadata (no blob data). Use AssetLoadActor
//! to load the actual data by ID.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_assets::{get_or_create_db, AssetQuery};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    AssetQueryActor,
    inports::<10>(trigger),
    outports::<1>(results, stats, error),
    state(MemoryState)
)]
pub async fn asset_query_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("dbPath")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");

    let db = get_or_create_db(db_path)?;

    // Build query from config
    let mut q = AssetQuery::new();

    if let Some(at) = config.get("assetType").and_then(|v| v.as_str()) {
        q = q.asset_type(at);
    }
    if let Some(name) = config.get("nameContains").and_then(|v| v.as_str()) {
        q = q.name_contains(name);
    }
    if let Some(tags) = config.get("tags").and_then(|v| v.as_array()) {
        for t in tags {
            if let Some(s) = t.as_str() {
                q = q.tag(s);
            }
        }
    }
    if config
        .get("matchAllTags")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        q = q.match_all_tags();
    }
    if let Some(limit) = config.get("limit").and_then(|v| v.as_u64()) {
        q = q.limit(limit as usize);
    }

    let entries = db.query(&q)?;

    let results: Vec<Value> = entries
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "name": e.name,
                "assetType": e.asset_type,
                "blobSize": e.blob_size,
                "tags": e.tags,
                "metadata": e.metadata,
                "createdAt": e.created_at,
            })
        })
        .collect();

    let stats = db.stats()?;

    let mut out = HashMap::new();
    out.insert(
        "results".to_string(),
        Message::object(EncodableValue::from(json!({
            "count": results.len(),
            "assets": results,
        }))),
    );
    out.insert(
        "stats".to_string(),
        Message::object(EncodableValue::from(stats)),
    );
    Ok(out)
}
