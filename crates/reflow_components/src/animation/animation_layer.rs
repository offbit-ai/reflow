//! Animation Layer — applies a partial/additive animation on top of a base pose.
//!
//! Supports per-bone masking so the layer only affects specific body parts
//! (e.g., upper body aiming while lower body runs).
//!
//! ## Modes
//! - `"override"` — replaces base transforms for masked bones (weight blended)
//! - `"additive"` — adds layer delta on top of base transforms
//!
//! ## Config
//! ```json
//! {
//!   "mode": "override",
//!   "weight": 1.0,
//!   "mask": [0,1,2,3,4,5]  // bone indices affected by this layer
//! }
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

use super::math_helpers::MAT4_IDENTITY;

#[actor(
    AnimationLayerActor,
    inports::<10>(base, layer, weight),
    outports::<1>(bone_transforms, metadata),
    state(MemoryState),
    await_inports(base)
)]
pub async fn animation_layer_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let mode = config
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("override");

    let w = match payload.get("weight") {
        Some(Message::Float(f)) => *f as f32,
        _ => config.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
    };

    let mask: Option<Vec<usize>> = config.get("mask").and_then(|v| v.as_array()).map(|a| {
        a.iter()
            .filter_map(|v| v.as_u64().map(|n| n as usize))
            .collect()
    });

    // Cache layer input
    if let Some(Message::Bytes(b)) = payload.get("layer") {
        use base64::Engine;
        ctx.pool_upsert(
            "_cache",
            "layer_b64",
            json!(base64::engine::general_purpose::STANDARD.encode(&**b)),
        );
    }

    let base_bytes = match payload.get("base") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Ok(HashMap::new()),
    };

    let layer_bytes: Option<Vec<u8>> = ctx
        .get_pool("_cache")
        .into_iter()
        .find(|(k, _)| k == "layer_b64")
        .and_then(|(_, v)| {
            v.as_str().map(|s| {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD
                    .decode(s)
                    .unwrap_or_default()
            })
        });

    let bone_count = base_bytes.len() / 64;

    let output = if let Some(layer) = layer_bytes {
        let layer_bones = layer.len() / 64;
        let count = bone_count.min(layer_bones);
        let mut out = base_bytes.clone();

        for b in 0..count {
            // Check mask
            if let Some(ref m) = mask {
                if !m.contains(&b) {
                    continue;
                }
            }

            let off = b * 64;
            match mode {
                "additive" => {
                    for j in 0..16 {
                        let base_v = f32::from_le_bytes(
                            base_bytes[off + j * 4..off + j * 4 + 4].try_into().unwrap(),
                        );
                        let layer_v = f32::from_le_bytes(
                            layer[off + j * 4..off + j * 4 + 4].try_into().unwrap(),
                        );
                        let id_v = MAT4_IDENTITY[j];
                        let result = base_v + w * (layer_v - id_v);
                        out[off + j * 4..off + j * 4 + 4].copy_from_slice(&result.to_le_bytes());
                    }
                }
                _ => {
                    // Override
                    for j in 0..16 {
                        let base_v = f32::from_le_bytes(
                            base_bytes[off + j * 4..off + j * 4 + 4].try_into().unwrap(),
                        );
                        let layer_v = f32::from_le_bytes(
                            layer[off + j * 4..off + j * 4 + 4].try_into().unwrap(),
                        );
                        let result = base_v * (1.0 - w) + layer_v * w;
                        out[off + j * 4..off + j * 4 + 4].copy_from_slice(&result.to_le_bytes());
                    }
                }
            }
        }
        out
    } else {
        base_bytes
    };

    let mut out = HashMap::new();
    out.insert("bone_transforms".to_string(), Message::bytes(output));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "boneCount": bone_count,
            "mode": mode,
            "weight": w,
            "masked": mask.is_some(),
        }))),
    );
    Ok(out)
}
