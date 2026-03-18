//! Gaussian blur actor — full-frame blur using reflow_pixel kernel.
//!
//! ## Inports
//! - `image` — RGBA bytes
//!
//! ## Config
//! ```json
//! { "radius": 5, "width": 512, "height": 512 }
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    GaussianBlurActor,
    inports::<10>(image),
    outports::<1>(image, metadata),
    state(MemoryState),
    await_inports(image)
)]
pub async fn gaussian_blur_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let mut data = match payload.get("image") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Err(anyhow::anyhow!("Expected Bytes on image port")),
    };

    let width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(512) as usize;
    let height = config
        .get("height")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| (data.len() / (width * 4)) as u64) as usize;
    let radius = config.get("radius").and_then(|v| v.as_u64()).unwrap_or(3) as usize;

    reflow_pixel::blur::gaussian_blur(&mut data, width, height, radius);

    let mut out = HashMap::new();
    out.insert("image".to_string(), Message::bytes(data));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width,
            "height": height,
            "radius": radius,
        }))),
    );
    Ok(out)
}
