//! SDF text system — converts text into SDF IR for 3D manipulation.
//!
//! This bridges the text pipeline with the SDF pipeline. Each glyph
//! becomes an SDF primitive that can be extruded, bent, booleaned,
//! and rendered via SDF ray marching.
//!
//! ## Component schema: `entity:text_sdf`
//!
//! ```json
//! {
//!   "content": "HELLO",
//!   "font": "impact:font",
//!   "fontSize": 2.0,
//!   "depth": 0.5,
//!   "bevel": 0.02,
//!   "letterSpacing": 0.1,
//!   "align": "center",
//!   "material": {
//!     "color": [0.9, 0.1, 0.1],
//!     "metallic": 0.8,
//!     "roughness": 0.2
//!   }
//! }
//! ```
//!
//! ## Output
//!
//! Writes `:sdf_text_field` — a 3D SDF volume (binary) that the SDF
//! renderer can ray march. Each glyph is a 2D SDF extruded to `depth`,
//! with optional bevel. The result is a single combined SDF of the
//! entire text string.
//!
//! Also writes `:sdf_text_ir` — SDF IR JSON that can be composed with
//! other SDF nodes (union with shapes, smooth blend, etc.).

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    TextSdfSystemActor,
    inports::<10>(tick, entity_id),
    outports::<1>(sdf_ir, metadata),
    state(MemoryState)
)]
pub async fn text_sdf_system_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");
    let db = get_or_create_db(db_path)?;

    let selected = super::selector::resolve_entities(&payload, &config, &db);
    let text_entities = if selected.is_empty() {
        db.entities_with(&["text_sdf"])?
    } else {
        selected
            .into_iter()
            .filter(|e| db.has_component(e, "text_sdf"))
            .collect()
    };

    let mut processed = 0;
    let mut last_ir: Option<Value> = None;

    for entity in &text_entities {
        let text_asset = match db.get_component(entity, "text_sdf") {
            Ok(a) => a,
            Err(_) => continue,
        };
        let data: Value = text_asset
            .entry
            .inline_data
            .unwrap_or_else(|| serde_json::from_slice(&text_asset.data).unwrap_or(json!({})));

        let content = data.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if content.is_empty() {
            continue;
        }

        let font_id = data
            .get("font")
            .and_then(|v| v.as_str())
            .unwrap_or("default:font");
        let font_size = data.get("fontSize").and_then(|v| v.as_f64()).unwrap_or(1.0);
        let depth = data.get("depth").and_then(|v| v.as_f64()).unwrap_or(0.3);
        let bevel = data.get("bevel").and_then(|v| v.as_f64()).unwrap_or(0.02);
        let letter_spacing = data
            .get("letterSpacing")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.05);
        let align = data
            .get("align")
            .and_then(|v| v.as_str())
            .unwrap_or("center");
        let material = data.get("material").cloned().unwrap_or(json!({}));

        // Load font
        let font_bytes = match db.get(font_id) {
            Ok(asset) => asset.data,
            Err(_) => continue,
        };

        let font = match fontdue::Font::from_bytes(
            font_bytes.as_slice(),
            fontdue::FontSettings::default(),
        ) {
            Ok(f) => f,
            Err(_) => continue,
        };

        // Build SDF IR: each glyph → 2D SDF extruded to depth
        let raster_size = 64.0f32; // Rasterize at this size for SDF generation
        let scale = font_size as f32 / raster_size;
        let mut glyph_nodes: Vec<Value> = Vec::new();
        let mut cursor_x: f64;

        // First pass: compute total width for alignment
        let mut total_width = 0.0f64;
        for ch in content.chars() {
            if ch == ' ' {
                total_width += font_size * 0.3 + letter_spacing;
                continue;
            }
            let (metrics, _) = font.rasterize(ch, raster_size);
            total_width += metrics.advance_width as f64 * scale as f64 + letter_spacing;
        }

        let align_offset = match align {
            "center" => -total_width / 2.0,
            "right" => -total_width,
            _ => 0.0,
        };
        cursor_x = align_offset;

        // Second pass: generate SDF IR nodes
        for ch in content.chars() {
            if ch == ' ' {
                cursor_x += font_size * 0.3 + letter_spacing;
                continue;
            }

            let (metrics, bitmap) = font.rasterize(ch, raster_size);
            if metrics.width == 0 || metrics.height == 0 {
                cursor_x += metrics.advance_width as f64 * scale as f64 + letter_spacing;
                continue;
            }

            // Generate 2D SDF from glyph bitmap
            let glyph_sdf = glyph_bitmap_to_sdf(&bitmap, metrics.width, metrics.height);
            let gw = metrics.width as f64 * scale as f64;
            let gh = metrics.height as f64 * scale as f64;
            let bearing_x = metrics.xmin as f64 * scale as f64;
            let bearing_y = metrics.ymin as f64 * scale as f64;

            // SDF IR: extruded 2D glyph
            // Each glyph is a box-like shape positioned along the text baseline
            let glyph_node = json!({
                "type": "glyph_extrude",
                "char": ch.to_string(),
                "sdf_data": base64_encode(&glyph_sdf),
                "sdf_width": metrics.width,
                "sdf_height": metrics.height,
                "position": [cursor_x + bearing_x + gw / 2.0, bearing_y + gh / 2.0, 0.0],
                "size": [gw, gh, depth],
                "bevel": bevel,
                "scale": scale,
            });

            glyph_nodes.push(glyph_node);
            cursor_x += metrics.advance_width as f64 * scale as f64 + letter_spacing;
        }

        // Combine all glyphs into a single SDF IR (union)
        let combined_ir = if glyph_nodes.len() == 1 {
            glyph_nodes[0].clone()
        } else {
            json!({
                "type": "union",
                "children": glyph_nodes,
            })
        };

        // Apply material
        let final_ir = if !material.is_null() && material.is_object() {
            json!({
                "type": "material",
                "color": material.get("color").unwrap_or(&json!([1, 1, 1])),
                "metallic": material.get("metallic").and_then(|v| v.as_f64()).unwrap_or(0.0),
                "roughness": material.get("roughness").and_then(|v| v.as_f64()).unwrap_or(0.5),
                "child": combined_ir,
            })
        } else {
            combined_ir
        };

        // IR is ephemeral — flows through DAG, not persisted to AssetDB.
        // Only the source :text_sdf component (prefab data) is stored.
        last_ir = Some(final_ir);
        processed += 1;
    }

    let mut out = HashMap::new();
    if let Some(ir) = last_ir {
        out.insert(
            "sdf_ir".to_string(),
            Message::object(EncodableValue::from(ir)),
        );
    }
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "textSdfProcessed": processed,
        }))),
    );
    Ok(out)
}

/// Convert a glyph bitmap to a 2D distance field.
fn glyph_bitmap_to_sdf(bitmap: &[u8], w: usize, h: usize) -> Vec<u8> {
    let padding = 4;
    let pw = w + padding * 2;
    let ph = h + padding * 2;
    let spread = padding as f32;
    let mut sdf = vec![128u8; pw * ph];

    for sy in 0..ph {
        for sx in 0..pw {
            let gx = sx as i32 - padding as i32;
            let gy = sy as i32 - padding as i32;

            let inside = if gx >= 0 && gx < w as i32 && gy >= 0 && gy < h as i32 {
                bitmap[gy as usize * w + gx as usize] > 127
            } else {
                false
            };

            let mut min_dist_sq = f32::MAX;
            let search = (spread as i32 + 1).max(2);

            for dy in -search..=search {
                for dx in -search..=search {
                    let nx = gx + dx;
                    let ny = gy + dy;
                    if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        let neighbor = bitmap[ny as usize * w + nx as usize] > 127;
                        if neighbor != inside {
                            min_dist_sq = min_dist_sq.min((dx * dx + dy * dy) as f32);
                        }
                    }
                }
            }

            let dist = min_dist_sq.sqrt();
            let signed = if inside { dist } else { -dist };
            sdf[sy * pw + sx] = (signed / spread * 127.0 + 128.0).clamp(0.0, 255.0) as u8;
        }
    }
    sdf
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}
