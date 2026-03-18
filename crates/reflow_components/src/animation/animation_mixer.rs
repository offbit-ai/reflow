//! Animation mixer — blends two bone transform sets.
//!
//! Supports linear blend and additive modes.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    AnimationMixerActor,
    inports::<10>(transforms_a, transforms_b, blend_weight),
    outports::<1>(bone_transforms, metadata),
    state(MemoryState),
    await_all_inports
)]
pub async fn animation_mixer_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let mode = config
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("blend");

    let weight = match payload.get("blend_weight") {
        Some(Message::Float(f)) => *f as f32,
        _ => config
            .get("defaultWeight")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5) as f32,
    };

    let bytes_a = match payload.get("transforms_a") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Err(anyhow::anyhow!("Expected transforms_a bytes")),
    };
    let bytes_b = match payload.get("transforms_b") {
        Some(Message::Bytes(b)) => b.to_vec(),
        _ => return Err(anyhow::anyhow!("Expected transforms_b bytes")),
    };

    let bone_count = bytes_a.len() / 64;
    let bone_count_b = bytes_b.len() / 64;
    let count = bone_count.min(bone_count_b);

    let mut output = Vec::with_capacity(count * 64);

    for i in 0..count {
        let off = i * 64;
        let mut ma = [0.0f32; 16];
        let mut mb = [0.0f32; 16];
        for j in 0..16 {
            ma[j] = f32::from_le_bytes(bytes_a[off + j * 4..off + j * 4 + 4].try_into().unwrap());
            mb[j] = f32::from_le_bytes(bytes_b[off + j * 4..off + j * 4 + 4].try_into().unwrap());
        }

        let blended = match mode {
            "additive" => {
                // Additive: result = a + weight * (b - identity)
                let mut r = [0.0f32; 16];
                let id = super::math_helpers::MAT4_IDENTITY;
                for j in 0..16 {
                    r[j] = ma[j] + weight * (mb[j] - id[j]);
                }
                r
            }
            _ => {
                // Linear blend
                let mut r = [0.0f32; 16];
                for j in 0..16 {
                    r[j] = ma[j] * (1.0 - weight) + mb[j] * weight;
                }
                r
            }
        };

        for f in &blended {
            output.extend_from_slice(&f.to_le_bytes());
        }
    }

    let mut out = HashMap::new();
    out.insert("bone_transforms".to_string(), Message::bytes(output));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "boneCount": count,
            "mode": mode,
            "weight": weight,
        }))),
    );
    Ok(out)
}
