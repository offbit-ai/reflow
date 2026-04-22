//! Animation blend tree — hierarchical multi-animation blending.
//!
//! Accepts multiple bone transform inputs and a blend parameter (0..1).
//! Config defines the tree structure: 1D blend space, 2D blend space, or direct blend.
//!
//! ## Modes
//! - `"1d"` — 1D blend space: parameter selects between ordered clips by threshold
//! - `"2d"` — 2D blend space: paramX/paramY for directional movement blending
//! - `"direct"` — explicit weights per input (weights array in config or via port)
//!
//! ## Inports
//! - `input_0..input_7` — up to 8 bone transform sets (Bytes, 64 bytes/bone)
//! - `parameter` — blend parameter (Float 0..1 for 1D, Object {x,y} for 2D)
//!
//! ## Outports
//! - `bone_transforms` — blended result

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    AnimationBlendTreeActor,
    inports::<10>(input_0, input_1, input_2, input_3, input_4, input_5, input_6, input_7, parameter),
    outports::<1>(bone_transforms, metadata),
    state(MemoryState),
    await_inports(parameter)
)]
pub async fn animation_blend_tree_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let mode = config.get("mode").and_then(|v| v.as_str()).unwrap_or("1d");

    // Cache incoming transform sets
    for i in 0..8 {
        let port = format!("input_{}", i);
        if let Some(Message::Bytes(b)) = payload.get(&port) {
            use base64::Engine;
            ctx.pool_upsert(
                "_inputs",
                &port,
                json!(base64::engine::general_purpose::STANDARD.encode(&**b)),
            );
        }
    }

    // Collect cached inputs
    let cache: HashMap<String, Value> = ctx.get_pool("_inputs").into_iter().collect();
    let mut inputs: Vec<(usize, Vec<u8>)> = Vec::new();
    for i in 0..8 {
        let key = format!("input_{}", i);
        if let Some(s) = cache.get(&key).and_then(|v| v.as_str()) {
            use base64::Engine;
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(s) {
                if !bytes.is_empty() {
                    inputs.push((i, bytes));
                }
            }
        }
    }

    if inputs.is_empty() {
        return Ok(HashMap::new());
    }

    let bone_count = inputs[0].1.len() / 64;

    // Get thresholds from config (for 1D mode)
    let thresholds: Vec<f64> = config
        .get("thresholds")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
        .unwrap_or_else(|| {
            let n = inputs.len();
            (0..n).map(|i| i as f64 / (n - 1).max(1) as f64).collect()
        });

    let output = match mode {
        "1d" => {
            let param = match payload.get("parameter") {
                Some(Message::Float(f)) => *f as f32,
                Some(Message::Integer(i)) => *i as f32,
                _ => 0.5,
            };
            blend_1d(&inputs, &thresholds, param, bone_count)
        }
        "2d" => {
            let (px, py) = match payload.get("parameter") {
                Some(Message::Object(obj)) => {
                    let v: Value = obj.as_ref().clone().into();
                    (
                        v.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                        v.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    )
                }
                _ => (0.0, 0.0),
            };
            let positions: Vec<[f32; 2]> = config
                .get("positions")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .map(|p| {
                            let arr = p.as_array();
                            let x = arr
                                .and_then(|a| a.first())
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0) as f32;
                            let y = arr
                                .and_then(|a| a.get(1))
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0) as f32;
                            [x, y]
                        })
                        .collect()
                })
                .unwrap_or_default();
            blend_2d(&inputs, &positions, [px, py], bone_count)
        }
        _ => {
            // Direct: explicit weights
            let weights: Vec<f32> = match payload.get("parameter") {
                Some(Message::Object(obj)) => {
                    let v: Value = obj.as_ref().clone().into();
                    v.get("weights")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_f64().map(|f| f as f32))
                                .collect()
                        })
                        .unwrap_or_default()
                }
                _ => config
                    .get("weights")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_f64().map(|f| f as f32))
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            blend_direct(&inputs, &weights, bone_count)
        }
    };

    let mut out = HashMap::new();
    out.insert("bone_transforms".to_string(), Message::bytes(output));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "boneCount": bone_count,
            "mode": mode,
            "inputCount": inputs.len(),
        }))),
    );
    Ok(out)
}

/// 1D blend space: interpolate between two nearest clips based on parameter threshold
fn blend_1d(
    inputs: &[(usize, Vec<u8>)],
    thresholds: &[f64],
    param: f32,
    bone_count: usize,
) -> Vec<u8> {
    if inputs.len() == 1 {
        return inputs[0].1.clone();
    }

    // Find the two clips that bracket the parameter
    let mut lower = 0;
    let mut upper = inputs.len() - 1;
    for i in 0..inputs.len() {
        let t = thresholds
            .get(i)
            .copied()
            .unwrap_or(i as f64 / (inputs.len() - 1) as f64) as f32;
        if t <= param {
            lower = i;
        }
        if t >= param && upper == inputs.len() - 1 {
            upper = i;
        }
    }
    if lower == upper {
        return inputs[lower].1.clone();
    }

    let t_low = thresholds.get(lower).copied().unwrap_or(0.0) as f32;
    let t_high = thresholds.get(upper).copied().unwrap_or(1.0) as f32;
    let range = t_high - t_low;
    let alpha = if range > 1e-6 {
        (param - t_low) / range
    } else {
        0.0
    };

    lerp_transforms(&inputs[lower].1, &inputs[upper].1, alpha, bone_count)
}

/// 2D blend space: inverse-distance weighted blend from positioned clips
fn blend_2d(
    inputs: &[(usize, Vec<u8>)],
    positions: &[[f32; 2]],
    target: [f32; 2],
    bone_count: usize,
) -> Vec<u8> {
    if inputs.len() == 1 {
        return inputs[0].1.clone();
    }

    let mut weights = Vec::with_capacity(inputs.len());
    let mut total = 0.0f32;
    for (i, _) in inputs.iter().enumerate() {
        let pos = positions.get(i).copied().unwrap_or([0.0, 0.0]);
        let dx = target[0] - pos[0];
        let dy = target[1] - pos[1];
        let dist = (dx * dx + dy * dy).sqrt().max(0.001);
        let w = 1.0 / dist;
        weights.push(w);
        total += w;
    }
    if total > 0.0 {
        for w in &mut weights {
            *w /= total;
        }
    }

    blend_direct_impl(inputs, &weights, bone_count)
}

/// Direct blend: explicit weights per input
fn blend_direct(inputs: &[(usize, Vec<u8>)], weights: &[f32], bone_count: usize) -> Vec<u8> {
    blend_direct_impl(inputs, weights, bone_count)
}

fn blend_direct_impl(inputs: &[(usize, Vec<u8>)], weights: &[f32], bone_count: usize) -> Vec<u8> {
    let mut output = vec![0u8; bone_count * 64];
    for (i, (_, bytes)) in inputs.iter().enumerate() {
        let w = weights.get(i).copied().unwrap_or(0.0);
        if w < 1e-6 {
            continue;
        }
        for b in 0..bone_count {
            let off = b * 64;
            for j in 0..16 {
                let src =
                    f32::from_le_bytes(bytes[off + j * 4..off + j * 4 + 4].try_into().unwrap());
                let dst =
                    f32::from_le_bytes(output[off + j * 4..off + j * 4 + 4].try_into().unwrap());
                let blended = dst + src * w;
                output[off + j * 4..off + j * 4 + 4].copy_from_slice(&blended.to_le_bytes());
            }
        }
    }
    output
}

fn lerp_transforms(a: &[u8], b: &[u8], alpha: f32, bone_count: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(bone_count * 64);
    for i in 0..bone_count {
        let off = i * 64;
        for j in 0..16 {
            let va = f32::from_le_bytes(a[off + j * 4..off + j * 4 + 4].try_into().unwrap());
            let vb = f32::from_le_bytes(b[off + j * 4..off + j * 4 + 4].try_into().unwrap());
            let r = va * (1.0 - alpha) + vb * alpha;
            output.extend_from_slice(&r.to_le_bytes());
        }
    }
    output
}
