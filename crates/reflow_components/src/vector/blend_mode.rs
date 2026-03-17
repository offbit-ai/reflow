//! Blend mode actor — composites two images with configurable blending.
//!
//! ## Inports
//! - `base` — background RGBA bytes
//! - `overlay` — foreground RGBA bytes
//!
//! ## Config
//! ```json
//! { "mode": "multiply", "opacity": 0.8 }
//! ```
//!
//! Modes: normal, multiply, screen, overlay, add, softLight, hardLight,
//! difference, exclusion, colorDodge, colorBurn, darken, lighten, subtract, divide

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
    state(MemoryState),
    await_inports(base, overlay)
)]
pub async fn blend_mode_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let mut base_data = match payload.get("base") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Err(anyhow::anyhow!("Expected Bytes on base port")),
    };

    let overlay_data = match payload.get("overlay") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Err(anyhow::anyhow!("Expected Bytes on overlay port")),
    };

    let mode_str = config.get("mode").and_then(|v| v.as_str()).unwrap_or("normal");
    let opacity = config.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;

    let mode = <reflow_pixel::blend::BlendMode>::from_str(mode_str);

    // Blend overlay onto base (lengths must match)
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
