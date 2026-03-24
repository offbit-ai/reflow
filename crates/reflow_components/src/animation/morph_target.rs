//! Morph Target / Blend Shape — deforms mesh vertices between shape keys.
//!
//! Takes a base mesh and one or more morph targets (deltas), blends them
//! by weight to produce the final deformed mesh. Used for facial expressions,
//! corrective shapes, etc.
//!
//! ## Config
//! ```json
//! { "stride": 24, "targetCount": 3 }
//! ```
//!
//! ## Inports
//! - `mesh` — base mesh bytes (cached)
//! - `target_0..target_7` — morph target delta bytes (same stride, cached)
//! - `weights` — Object { weights: [0.5, 0.3, 0.0, ...] } per frame
//!
//! ## Outports
//! - `mesh` — deformed mesh

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    MorphTargetActor,
    inports::<10>(mesh, target_0, target_1, target_2, target_3, target_4, target_5, target_6, target_7, weights),
    outports::<1>(mesh, metadata),
    state(MemoryState),
    await_inports(weights)
)]
pub async fn morph_target_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let stride = config.get("stride").and_then(|v| v.as_u64()).unwrap_or(24) as usize;

    // Cache base mesh and targets
    if let Some(Message::Bytes(b)) = payload.get("mesh") {
        use base64::Engine;
        ctx.pool_upsert("_morph", "base", json!(base64::engine::general_purpose::STANDARD.encode(&**b)));
    }
    for i in 0..8 {
        let port = format!("target_{}", i);
        if let Some(Message::Bytes(b)) = payload.get(&port) {
            use base64::Engine;
            ctx.pool_upsert("_morph", &port, json!(base64::engine::general_purpose::STANDARD.encode(&**b)));
        }
    }

    // Get weights
    let weights: Vec<f32> = match payload.get("weights") {
        Some(Message::Object(obj)) => {
            let v: Value = obj.as_ref().clone().into();
            v.get("weights")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
                .unwrap_or_default()
        }
        _ => return Ok(HashMap::new()),
    };

    // Read cached data
    let cache: HashMap<String, Value> = ctx.get_pool("_morph").into_iter().collect();

    let base_mesh = match cache.get("base").and_then(|v| v.as_str()) {
        Some(s) => {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.decode(s).unwrap_or_default()
        }
        None => return Ok(HashMap::new()),
    };

    let vertex_count = base_mesh.len() / stride;
    // Only morph position components (first 12 bytes = xyz f32)
    let pos_floats = 3;

    let mut output = base_mesh.clone();

    for (ti, &w) in weights.iter().enumerate() {
        if w.abs() < 1e-6 { continue; }
        let key = format!("target_{}", ti);
        let target = match cache.get(&key).and_then(|v| v.as_str()) {
            Some(s) => {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.decode(s).unwrap_or_default()
            }
            None => continue,
        };
        if target.len() != base_mesh.len() { continue; }

        // Add weighted delta to position components
        for vi in 0..vertex_count {
            let off = vi * stride;
            for j in 0..pos_floats {
                let fo = off + j * 4;
                let base_v = f32::from_le_bytes(output[fo..fo + 4].try_into().unwrap());
                let target_v = f32::from_le_bytes(target[fo..fo + 4].try_into().unwrap());
                let delta = target_v - f32::from_le_bytes(base_mesh[fo..fo + 4].try_into().unwrap());
                let result = base_v + delta * w;
                output[fo..fo + 4].copy_from_slice(&result.to_le_bytes());
            }
        }
    }

    let mut out = HashMap::new();
    out.insert("mesh".to_string(), Message::bytes(output));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "vertexCount": vertex_count,
            "activeTargets": weights.iter().filter(|w| w.abs() > 1e-6).count(),
        }))),
    );
    Ok(out)
}
