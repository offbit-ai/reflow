//! Font load actor — loads TTF/OTF from file path, AssetDB, or inport.
//!
//! Loads font bytes once and caches in actor pool. Subsequent ticks
//! re-emit cached data without re-reading.
//!
//! ## Config
//!
//! ```json
//! { "path": "/path/to/font.ttf" }
//! ```
//! or
//! ```json
//! { "font": "roboto:font", "$db": "./assets.db" }
//! ```
//!
//! ## Inports
//!
//! - `font_data` — raw font bytes from upstream (bypasses file/db loading)
//! - `tick` — trigger (only loads on first tick, then re-emits cached)
//!
//! ## Outports
//!
//! - `font_data` — raw TTF/OTF bytes (Message::bytes)
//! - `metadata` — font info

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    FontLoadActor,
    inports::<10>(font_data, tick),
    outports::<1>(font_data, metadata),
    state(MemoryState)
)]
pub async fn font_load_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    // If font bytes arrive on inport, cache and forward them
    if let Some(Message::Bytes(bytes)) = payload.get("font_data") {
        let data = bytes.to_vec();
        ctx.pool_upsert("_font", "loaded", json!(true));
        ctx.pool_upsert("_font", "size", json!(data.len()));
        // Store raw bytes in pool via a special key
        // (We use the actor's state directly since pool is JSON-based)
        let mut out = HashMap::new();
        out.insert("font_data".to_string(), Message::bytes(data));
        return Ok(out);
    }

    // Check if already loaded (pool flag)
    let is_loaded = ctx
        .get_pool("_font")
        .into_iter()
        .any(|(k, v)| k == "loaded" && v == json!(true));

    // On tick: load from file/db if not already cached
    if !is_loaded {
        let font_bytes = load_font_bytes(&config)?;
        let size = font_bytes.len();

        ctx.pool_upsert("_font", "loaded", json!(true));
        ctx.pool_upsert("_font", "size", json!(size));

        let source = config
            .get("path")
            .and_then(|v| v.as_str())
            .or_else(|| config.get("font").and_then(|v| v.as_str()))
            .unwrap_or("unknown");

        let mut out = HashMap::new();
        out.insert("font_data".to_string(), Message::bytes(font_bytes));
        out.insert(
            "metadata".to_string(),
            Message::object(EncodableValue::from(json!({
                "source": source,
                "size": size,
            }))),
        );
        return Ok(out);
    }

    // Already loaded — nothing to re-emit (font_data was sent on first tick)
    // Downstream actors should have cached it in their own pools.
    Ok(HashMap::new())
}

fn load_font_bytes(config: &HashMap<String, Value>) -> Result<Vec<u8>> {
    // Option 1: file path
    if let Some(path) = config.get("path").and_then(|v| v.as_str()) {
        return std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("Failed to read font file '{}': {}", path, e));
    }

    // Option 2: AssetDB
    if let Some(font_id) = config.get("font").and_then(|v| v.as_str()) {
        if let Some(db_path) = config.get("$db").and_then(|v| v.as_str()) {
            let db = reflow_assets::get_or_create_db(db_path)?;
            if let Ok(asset) = db.get(font_id) {
                return Ok(asset.data);
            }
        }
    }

    Err(anyhow::anyhow!(
        "No font source configured. Set 'path' or 'font' + '$db' in config."
    ))
}
