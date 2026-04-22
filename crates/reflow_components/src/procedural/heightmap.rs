//! Heightmap conversion actors.
//!
//! - ImageToHeightmap: extract luminance from RGBA image bytes → f64 grid
//! - HeightmapToImage: render f64 grid → grayscale RGBA image bytes
//! - NoiseToImage: same as HeightmapToImage (alias for clarity in graphs)

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

// ── Image → Heightmap ───────────────────────────────────────────

/// Converts RGBA image bytes to a height grid (f64 per pixel, 0.0–1.0).
/// Uses luminance: 0.299R + 0.587G + 0.114B.
#[actor(ImageToHeightmapActor, inports::<10>(input), outports::<1>(output, metadata, error), state(MemoryState))]
pub async fn image_to_heightmap_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let bytes = match payload.get("input") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on input port")),
    };

    // Try to decode as an image
    let img = match image::load_from_memory(&bytes) {
        Ok(img) => img,
        Err(_) => {
            // Assume raw RGBA — infer dimensions
            let channels = config.get("channels").and_then(|v| v.as_u64()).unwrap_or(4) as usize;
            let total_pixels = bytes.len() / channels;
            let width = config
                .get("width")
                .and_then(|v| v.as_u64())
                .unwrap_or((total_pixels as f64).sqrt() as u64) as usize;
            let height = total_pixels / width;

            let mut grid = Vec::with_capacity(width * height);
            for i in 0..width * height {
                let offset = i * channels;
                if offset + 2 < bytes.len() {
                    let r = bytes[offset] as f64 / 255.0;
                    let g = bytes[offset + 1] as f64 / 255.0;
                    let b = bytes[offset + 2] as f64 / 255.0;
                    grid.push(0.299 * r + 0.587 * g + 0.114 * b);
                } else {
                    grid.push(0.0);
                }
            }

            let grid_bytes: Vec<u8> = grid.iter().flat_map(|v| v.to_le_bytes()).collect();
            let mut out = HashMap::new();
            out.insert("output".to_string(), Message::bytes(grid_bytes));
            out.insert(
                "metadata".to_string(),
                Message::object(EncodableValue::from(json!({
                    "width": width, "height": height, "dataType": "f64",
                }))),
            );
            return Ok(out);
        }
    };

    let (width, height) = (img.width() as usize, img.height() as usize);
    let rgba = img.to_rgba8();
    let pixels = rgba.as_raw();

    let mut grid = Vec::with_capacity(width * height);
    for i in 0..width * height {
        let r = pixels[i * 4] as f64 / 255.0;
        let g = pixels[i * 4 + 1] as f64 / 255.0;
        let b = pixels[i * 4 + 2] as f64 / 255.0;
        grid.push(0.299 * r + 0.587 * g + 0.114 * b);
    }

    let grid_bytes: Vec<u8> = grid.iter().flat_map(|v| v.to_le_bytes()).collect();
    let mut out = HashMap::new();
    out.insert("output".to_string(), Message::bytes(grid_bytes));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width, "height": height, "dataType": "f64",
            "min": grid.iter().cloned().fold(f64::MAX, f64::min),
            "max": grid.iter().cloned().fold(f64::MIN, f64::max),
        }))),
    );
    Ok(out)
}

// ── Heightmap → Image ───────────────────────────────────────────

/// Converts f64 height grid to grayscale RGBA image bytes.
/// Normalizes values to 0–255 range.
#[actor(HeightmapToImageActor, inports::<10>(input), outports::<1>(output, metadata, error), state(MemoryState))]
pub async fn heightmap_to_image_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let bytes = match payload.get("input") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on input port")),
    };

    // Parse f64 grid
    let grid: Vec<f64> = bytes
        .chunks_exact(8)
        .map(|b| f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
        .collect();

    let total = grid.len();
    let width = config
        .get("width")
        .and_then(|v| v.as_u64())
        .unwrap_or((total as f64).sqrt() as u64) as usize;
    let height = total / width;

    // Normalize to 0–1
    let min = grid.iter().cloned().fold(f64::MAX, f64::min);
    let max = grid.iter().cloned().fold(f64::MIN, f64::max);
    let range = if (max - min).abs() > 1e-10 {
        max - min
    } else {
        1.0
    };

    let color_mode = config
        .get("colorMode")
        .and_then(|v| v.as_str())
        .unwrap_or("grayscale");

    // Generate RGBA pixels
    let mut rgba = Vec::with_capacity(width * height * 4);
    for &v in &grid {
        let norm = ((v - min) / range).clamp(0.0, 1.0);
        match color_mode {
            "terrain" => {
                // Blue (water) → green (land) → brown (mountain) → white (snow)
                let (r, g, b) = if norm < 0.3 {
                    (30, 80, (180.0 + norm * 200.0) as u8)
                } else if norm < 0.6 {
                    let t = (norm - 0.3) / 0.3;
                    (
                        (30.0 + t * 100.0) as u8,
                        (120.0 + t * 60.0) as u8,
                        (40.0 + t * 30.0) as u8,
                    )
                } else if norm < 0.85 {
                    let t = (norm - 0.6) / 0.25;
                    (
                        (130.0 + t * 60.0) as u8,
                        (100.0 - t * 40.0) as u8,
                        (50.0 + t * 20.0) as u8,
                    )
                } else {
                    let t = (norm - 0.85) / 0.15;
                    let g = (200.0 + t * 55.0) as u8;
                    (g, g, g)
                };
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
            _ => {
                // Grayscale
                let v = (norm * 255.0) as u8;
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
    }

    let mut out = HashMap::new();
    out.insert("output".to_string(), Message::bytes(rgba));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width, "height": height,
            "format": "RGBA8", "channels": 4,
            "colorMode": color_mode,
        }))),
    );
    Ok(out)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
