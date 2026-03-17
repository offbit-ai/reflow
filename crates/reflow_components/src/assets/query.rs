//! Asset query actor — searches the AssetDB using document-style DSL.
//!
//! The query config IS the query — describe what you're looking for:
//!
//! ```json
//! { "type": "mesh", "tags": ["snake"], "$sort": "newest", "$limit": 5 }
//! { "name": { "$contains": "body" }, "metadata.stride": 24 }
//! { "type": { "$in": ["mesh", "texture"] }, "size": { "$gt": 10000 } }
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    AssetQueryActor,
    inports::<10>(trigger, query),
    outports::<1>(results, stats, error),
    state(MemoryState)
)]
pub async fn asset_query_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .or_else(|| config.get("dbPath"))
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");

    let db = get_or_create_db(db_path)?;

    // Query comes from config directly (the config IS the query),
    // or from the query inport for dynamic queries.
    let query_json: Value = if let Some(Message::Object(obj)) = payload.get("query") {
        obj.as_ref().clone().into()
    } else {
        // Build query from config, stripping control keys
        let mut q = serde_json::Map::new();
        for (k, v) in &config {
            if k == "dbPath" || k == "$db" {
                continue;
            }
            q.insert(k.clone(), v.clone());
        }
        Value::Object(q)
    };

    let entries = db.query_dsl(&query_json)?;

    let results: Vec<Value> = entries
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "name": e.name,
                "type": e.asset_type,
                "size": e.blob_size,
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
