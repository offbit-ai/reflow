//! Blend mode actor — composites two images with configurable blending.
//!
//! Caches both layers. Re-fires whenever either input updates.
//! For per-frame pipelines, both inputs arrive each tick.
//!
//! ## Config
//! ```json
//! { "mode": "multiply", "opacity": 0.8 }
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    BlendModeActor,
    inports::<10>(base, overlay),
    outports::<1>(image, metadata),
    state(MemoryState)
)]
pub async fn blend_mode_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    // Cache inputs in pool
    if let Some(Message::Bytes(b)) = payload.get("base") {
        let encoded = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(&**b)
        };
        ctx.pool_upsert("_blend", "base", json!(encoded));
    }
    if let Some(Message::Bytes(b)) = payload.get("overlay") {
        let encoded = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(&**b)
        };
        ctx.pool_upsert("_blend", "overlay", json!(encoded));
    }

    // Need both to composite
    let pool: HashMap<String, serde_json::Value> = ctx.get_pool("_blend").into_iter().collect();
    let base_b64 = match pool.get("base").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return Ok(HashMap::new()),
    };
    let overlay_b64 = match pool.get("overlay").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return Ok(HashMap::new()),
    };

    use base64::Engine;
    let mut base_data = base64::engine::general_purpose::STANDARD.decode(base_b64)?;
    let overlay_data = base64::engine::general_purpose::STANDARD.decode(overlay_b64)?;

    let mode_str = config.get("mode").and_then(|v| v.as_str()).unwrap_or("normal");
    let opacity = config.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;

    let mode = <reflow_pixel::blend::BlendMode>::from_str(mode_str);

    let len = base_data.len().min(overlay_data.len());
    reflow_pixel::blend::blend_rows(&mut base_data[..len], &overlay_data[..len], mode, opacity);

    let mut out = HashMap::new();
    out.insert("image".to_string(), Message::bytes(base_data));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "mode": mode_str,
            "opacity": opacity,
        }))),
    );
    Ok(out)
}
