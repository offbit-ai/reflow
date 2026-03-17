//! Glyph atlas actor — builds SDF glyph atlas from font data.
//!
//! Receives raw font bytes (from FontLoadActor), generates an SDF atlas,
//! and outputs the atlas bitmap + per-glyph metrics. Caches the atlas
//! in the global font cache so it persists across ticks.
//!
//! ## Config
//!
//! ```json
//! { "fontSize": 64, "sdf": true, "characters": "" }
//! ```
//!
//! ## Inports
//!
//! - `font_data` — raw TTF/OTF bytes (one-shot, cached in pool)
//! - `tick` — trigger re-emit of cached atlas
//!
//! ## Outports
//!
//! - `atlas` — single-channel SDF atlas bitmap (Message::bytes)
//! - `metrics` — per-glyph metrics as JSON object
//! - `atlas_size` — [width, height] of the atlas

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::{json, Value};
use std::collections::HashMap;

use super::font_atlas;

#[actor(
    GlyphAtlasActor,
    inports::<10>(font_data, tick),
    outports::<1>(atlas, metrics, atlas_size),
    state(MemoryState)
)]
pub async fn glyph_atlas_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let font_size = config.get("fontSize").and_then(|v| v.as_f64()).unwrap_or(64.0) as f32;
    let is_sdf = config.get("sdf").and_then(|v| v.as_bool()).unwrap_or(true);
    let extra_chars = config.get("characters").and_then(|v| v.as_str()).unwrap_or("");

    // Cache font bytes from inport (one-shot)
    if let Some(Message::Bytes(bytes)) = payload.get("font_data") {
        ctx.pool_upsert("_ga", "font_bytes_len", json!(bytes.len()));
        // Build atlas immediately when font arrives
        let atlas = font_atlas::get_or_build_atlas(
            "inport_font",
            bytes,
            font_size,
            is_sdf,
            extra_chars,
        )?;
        return emit_atlas(&ctx, &atlas);
    }

    // On tick: re-emit cached atlas if available
    let has_atlas = ctx.get_pool("_ga")
        .into_iter()
        .any(|(k, _)| k == "atlas_ready");

    if has_atlas {
        // Atlas is in the global cache — retrieve it
        if let Ok(atlas) = font_atlas::get_or_build_atlas(
            "inport_font", &[], font_size, is_sdf, extra_chars,
        ) {
            return emit_atlas(&ctx, &atlas);
        }
    }

    Ok(HashMap::new())
}

fn emit_atlas(
    ctx: &ActorContext,
    atlas: &font_atlas::FontAtlas,
) -> Result<HashMap<String, Message>, Error> {
    ctx.pool_upsert("_ga", "atlas_ready", json!(true));

    // Build metrics JSON
    let mut metrics_map = serde_json::Map::new();
    for (ch, info) in &atlas.glyphs {
        metrics_map.insert(ch.to_string(), json!({
            "x": info.atlas_x,
            "y": info.atlas_y,
            "w": info.width,
            "h": info.height,
            "advance": info.advance,
            "bearing_x": info.bearing_x,
            "bearing_y": info.bearing_y,
        }));
    }

    let mut out = HashMap::new();
    out.insert("atlas".to_string(), Message::bytes(atlas.bitmap.clone()));
    out.insert("metrics".to_string(), Message::object(
        EncodableValue::from(Value::Object(metrics_map)),
    ));
    out.insert("atlas_size".to_string(), Message::object(
        EncodableValue::from(json!([atlas.width, atlas.height])),
    ));
    Ok(out)
}
