//! Noise generator actor.
//!
//! Single actor with configurable noise type. Outputs a 2D noise grid
//! as Message::Bytes (f64 little-endian) plus metadata.
//!
//! Config: noiseType (perlin|simplex|worley|value|ridged|white),
//!         width, height, scale, offsetX, offsetY,
//!         octaves, lacunarity, persistence, seed

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

/// Unified noise generator. All parameters available as both
/// inports (connectable) and config (editable in property panel).
#[actor(
    NoiseGeneratorActor,
    inports::<10>(scale, seed, offsetX, offsetY),
    outports::<1>(output, metadata),
    state(MemoryState)
)]
pub async fn noise_generator_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let c = ctx.get_config_hashmap();
    let p = ctx.get_payload();

    let noise_type = c
        .get("noiseType")
        .and_then(|v| v.as_str())
        .unwrap_or("perlin")
        .to_string();

    let f = |port: &str, key: &str, default: f64| -> f64 {
        match p.get(port) {
            Some(Message::Float(v)) => return *v,
            Some(Message::Integer(v)) => return *v as f64,
            _ => {}
        }
        c.get(key).and_then(|v| v.as_f64()).unwrap_or(default)
    };

    let width = c.get("width").and_then(|v| v.as_u64()).unwrap_or(256) as usize;
    let height = c.get("height").and_then(|v| v.as_u64()).unwrap_or(256) as usize;
    let scale = f("scale", "scale", 4.0);
    let offset_x = f("offsetX", "offsetX", 0.0);
    let offset_y = f("offsetY", "offsetY", 0.0);
    let seed = f("seed", "seed", 0.0);
    let octaves = c.get("octaves").and_then(|v| v.as_u64()).unwrap_or(6) as u32;
    let lacunarity = c.get("lacunarity").and_then(|v| v.as_f64()).unwrap_or(2.0);
    let persistence = c.get("persistence").and_then(|v| v.as_f64()).unwrap_or(0.5);

    use reflow_sdf::noise;

    let grid = match noise_type.as_str() {
        "simplex" => noise::generate_noise_grid(width, height, scale, offset_x + seed, offset_y,
            |x, y| noise::fbm_2d(x, y, octaves, lacunarity, persistence, noise::simplex_2d)),
        "worley" => noise::generate_noise_grid(width, height, scale, offset_x + seed, offset_y,
            noise::worley_2d),
        "value" => noise::generate_noise_grid(width, height, scale, offset_x + seed, offset_y,
            noise::value_noise_2d),
        "ridged" => noise::generate_noise_grid(width, height, scale, offset_x + seed, offset_y,
            |x, y| noise::ridged_2d(x, y, octaves, lacunarity, persistence, noise::perlin_2d)),
        "white" => {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as f64 + seed;
            noise::generate_noise_grid(width, height, 1.0, t, 0.0,
                |x, y| noise::value_noise(x * 1000.0, y * 1000.0, t))
        }
        _ /* perlin */ => noise::generate_noise_grid(width, height, scale, offset_x + seed, offset_y,
            |x, y| noise::fbm_2d(x, y, octaves, lacunarity, persistence, noise::perlin_2d)),
    };

    let min = grid.iter().cloned().fold(f64::MAX, f64::min);
    let max = grid.iter().cloned().fold(f64::MIN, f64::max);
    let bytes: Vec<u8> = grid.iter().flat_map(|v| v.to_le_bytes()).collect();

    let mut out = HashMap::new();
    out.insert("output".to_string(), Message::bytes(bytes));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width,
            "height": height,
            "noiseType": noise_type,
            "scale": scale,
            "octaves": octaves,
            "min": min,
            "max": max,
            "dataType": "f64",
        }))),
    );
    Ok(out)
}
