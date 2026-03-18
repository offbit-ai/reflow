//! Text render system — lays out glyphs and generates renderable quads.
//!
//! ## Component schema: `entity:text`
//!
//! ```json
//! {
//!   "content": "Hello World",
//!   "font": "roboto:font",
//!   "fontSize": 24,
//!   "color": [1, 1, 1, 1],
//!   "align": "center",
//!   "baseline": "middle",
//!   "maxWidth": 300,
//!   "lineHeight": 1.2,
//!   "letterSpacing": 0,
//!   "outline": { "width": 2, "color": [0, 0, 0, 1] },
//!   "shadow": { "offset": [1, 1], "blur": 2, "color": [0, 0, 0, 0.5] },
//!   "sdf": false
//! }
//! ```
//!
//! When `sdf: true`, generates SDF glyphs instead of bitmap — allowing
//! smooth scaling, 3D extrusion, and SDF operations (boolean, blend).
//!
//! ## Output
//!
//! Writes `:text_quads` component with positioned glyph quads + atlas UV coords.
//! Writes `:text_atlas` as a binary bitmap (or SDF field) to the font entity.
//!
//! ## Font assets
//!
//! Fonts are stored in AssetDB as binary TTF/OTF data:
//!   `db.put("roboto:font", &ttf_bytes, json!({"family": "Roboto"}))`
//!
//! The system rasterizes glyphs on demand and caches the atlas.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

// ═══════════════════════════════════════════════════════════════════════════
// Font atlas cache — shared across ticks
// ═══════════════════════════════════════════════════════════════════════════

struct FontAtlas {
    /// Atlas bitmap: grayscale (1 byte per pixel) or SDF (1 byte per pixel)
    bitmap: Vec<u8>,
    width: u32,
    height: u32,
    /// Glyph metrics: char → (x, y, w, h, advance, bearing_x, bearing_y) in atlas
    glyphs: HashMap<char, GlyphInfo>,
    is_sdf: bool,
}

#[derive(Clone)]
struct GlyphInfo {
    atlas_x: u32,
    atlas_y: u32,
    width: u32,
    height: u32,
    advance: f32,
    bearing_x: f32,
    bearing_y: f32,
}

static FONT_CACHE: std::sync::OnceLock<RwLock<HashMap<String, Arc<FontAtlas>>>> =
    std::sync::OnceLock::new();

fn font_cache() -> &'static RwLock<HashMap<String, Arc<FontAtlas>>> {
    FONT_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn get_or_build_atlas(
    font_id: &str,
    font_data: &[u8],
    font_size: f32,
    is_sdf: bool,
    chars: &str,
) -> Result<Arc<FontAtlas>> {
    let cache_key = format!(
        "{}:{}:{}",
        font_id,
        font_size as u32,
        if is_sdf { "sdf" } else { "bmp" }
    );

    // Check cache
    if let Ok(cache) = font_cache().read() {
        if let Some(atlas) = cache.get(&cache_key) {
            return Ok(Arc::clone(atlas));
        }
    }

    // Build atlas
    let font = fontdue::Font::from_bytes(font_data, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("Failed to parse font: {}", e))?;

    // Collect unique chars
    let mut unique_chars: Vec<char> = chars.chars().collect();
    // Always include ASCII printable range
    for c in 32u8..=126 {
        let ch = c as char;
        if !unique_chars.contains(&ch) {
            unique_chars.push(ch);
        }
    }

    // Rasterize each glyph
    let sdf_padding = if is_sdf { 8u32 } else { 1 };
    let mut glyph_bitmaps: Vec<(char, Vec<u8>, fontdue::Metrics)> = Vec::new();

    for &ch in &unique_chars {
        let (metrics, bitmap) = if is_sdf {
            // Rasterize larger then generate SDF
            let (metrics, bitmap) = font.rasterize(ch, font_size);
            let sdf = generate_sdf(&bitmap, metrics.width, metrics.height, sdf_padding);
            let sdf_w = metrics.width + sdf_padding as usize * 2;
            let sdf_h = metrics.height + sdf_padding as usize * 2;
            let sdf_metrics = fontdue::Metrics {
                width: sdf_w,
                height: sdf_h,
                ..metrics
            };
            (sdf_metrics, sdf)
        } else {
            font.rasterize(ch, font_size)
        };
        glyph_bitmaps.push((ch, bitmap, metrics));
    }

    // Pack glyphs into atlas (simple row packing)
    let max_glyph_h = glyph_bitmaps
        .iter()
        .map(|(_, _, m)| m.height)
        .max()
        .unwrap_or(0);
    let total_width_estimate: usize = glyph_bitmaps.iter().map(|(_, _, m)| m.width + 2).sum();
    let atlas_width = ((total_width_estimate as f64).sqrt() * 1.5) as u32;
    let atlas_width = atlas_width.max(256).next_power_of_two();

    let mut atlas_height = max_glyph_h as u32 + 2;
    let mut cursor_x = 1u32;
    let mut cursor_y = 1u32;
    let mut row_height = 0u32;
    let mut glyphs = HashMap::new();

    // First pass: compute layout
    for (ch, _, metrics) in &glyph_bitmaps {
        let gw = metrics.width as u32;
        let gh = metrics.height as u32;

        if cursor_x + gw + 1 > atlas_width {
            cursor_x = 1;
            cursor_y += row_height + 1;
            row_height = 0;
        }

        glyphs.insert(
            *ch,
            GlyphInfo {
                atlas_x: cursor_x,
                atlas_y: cursor_y,
                width: gw,
                height: gh,
                advance: metrics.advance_width,
                bearing_x: metrics.xmin as f32,
                bearing_y: metrics.ymin as f32,
            },
        );

        cursor_x += gw + 1;
        row_height = row_height.max(gh);
    }

    atlas_height = (cursor_y + row_height + 1)
        .next_power_of_two()
        .max(atlas_height);

    // Second pass: blit glyphs into atlas
    let mut bitmap = vec![0u8; (atlas_width * atlas_height) as usize];

    for (ch, glyph_bmp, _) in &glyph_bitmaps {
        if let Some(info) = glyphs.get(ch) {
            for y in 0..info.height {
                for x in 0..info.width {
                    let src = (y * info.width + x) as usize;
                    let dst = ((info.atlas_y + y) * atlas_width + info.atlas_x + x) as usize;
                    if src < glyph_bmp.len() && dst < bitmap.len() {
                        bitmap[dst] = glyph_bmp[src];
                    }
                }
            }
        }
    }

    let atlas = Arc::new(FontAtlas {
        bitmap,
        width: atlas_width,
        height: atlas_height,
        glyphs,
        is_sdf,
    });

    if let Ok(mut cache) = font_cache().write() {
        cache.insert(cache_key, Arc::clone(&atlas));
    }

    Ok(atlas)
}

/// Generate an SDF from a binary glyph bitmap.
/// Uses brute-force distance computation (acceptable for small glyphs).
fn generate_sdf(bitmap: &[u8], w: usize, h: usize, padding: u32) -> Vec<u8> {
    let pw = w + padding as usize * 2;
    let ph = h + padding as usize * 2;
    let spread = padding as f32;
    let mut sdf = vec![0u8; pw * ph];

    for sy in 0..ph {
        for sx in 0..pw {
            let gx = sx as i32 - padding as i32;
            let gy = sy as i32 - padding as i32;

            // Is this pixel inside the glyph?
            let inside = if gx >= 0 && gx < w as i32 && gy >= 0 && gy < h as i32 {
                bitmap[gy as usize * w + gx as usize] > 127
            } else {
                false
            };

            // Find nearest boundary pixel
            let mut min_dist_sq = f32::MAX;
            let search = (spread as i32 + 1).max(2);

            for dy in -search..=search {
                for dx in -search..=search {
                    let nx = gx + dx;
                    let ny = gy + dy;
                    if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        let neighbor_inside = bitmap[ny as usize * w + nx as usize] > 127;
                        if neighbor_inside != inside {
                            let dist_sq = (dx * dx + dy * dy) as f32;
                            min_dist_sq = min_dist_sq.min(dist_sq);
                        }
                    }
                }
            }

            let dist = min_dist_sq.sqrt();
            let signed = if inside { dist } else { -dist };
            // Map to 0..255: 128 = boundary, >128 = inside, <128 = outside
            let normalized = (signed / spread * 127.0 + 128.0).clamp(0.0, 255.0) as u8;
            sdf[sy * pw + sx] = normalized;
        }
    }
    sdf
}

// ═══════════════════════════════════════════════════════════════════════════
// Text layout
// ═══════════════════════════════════════════════════════════════════════════

struct LayoutGlyph {
    ch: char,
    x: f32,
    y: f32,
    info: GlyphInfo,
}

fn layout_text(
    text: &str,
    atlas: &FontAtlas,
    font_size: f32,
    max_width: f32,
    line_height: f32,
    letter_spacing: f32,
    align: &str,
) -> (Vec<LayoutGlyph>, f32, f32) {
    let mut glyphs = Vec::new();
    let mut lines: Vec<(usize, usize, f32)> = Vec::new(); // (start_idx, end_idx, line_width)

    let mut cursor_x = 0.0f32;
    let mut cursor_y = 0.0f32;
    let actual_line_height = font_size * line_height;
    let mut line_start = 0;

    for ch in text.chars() {
        if ch == '\n' {
            lines.push((line_start, glyphs.len(), cursor_x));
            cursor_x = 0.0;
            cursor_y += actual_line_height;
            line_start = glyphs.len();
            continue;
        }

        let info = match atlas.glyphs.get(&ch) {
            Some(i) => i.clone(),
            None => match atlas.glyphs.get(&'?') {
                Some(i) => i.clone(),
                None => continue,
            },
        };

        // Word wrap
        if max_width > 0.0 && cursor_x + info.advance > max_width && cursor_x > 0.0 {
            lines.push((line_start, glyphs.len(), cursor_x));
            cursor_x = 0.0;
            cursor_y += actual_line_height;
            line_start = glyphs.len();
        }

        glyphs.push(LayoutGlyph {
            ch,
            x: cursor_x + info.bearing_x,
            y: cursor_y - info.bearing_y,
            info,
        });

        cursor_x += glyphs.last().unwrap().info.advance + letter_spacing;
    }

    // Last line
    lines.push((line_start, glyphs.len(), cursor_x));

    let total_height = cursor_y + actual_line_height;
    let total_width = lines.iter().map(|(_, _, w)| *w).fold(0.0f32, f32::max);

    // Apply alignment
    for &(start, end, line_w) in &lines {
        let offset = match align {
            "center" => (total_width - line_w) / 2.0,
            "right" => total_width - line_w,
            _ => 0.0,
        };
        if offset > 0.0 {
            for g in &mut glyphs[start..end] {
                g.x += offset;
            }
        }
    }

    (glyphs, total_width, total_height)
}

// ═══════════════════════════════════════════════════════════════════════════
// System actor
// ═══════════════════════════════════════════════════════════════════════════

#[actor(
    TextRenderSystemActor,
    inports::<10>(tick, entity_id),
    outports::<1>(text_quads, metadata),
    state(MemoryState)
)]
pub async fn text_render_system_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");
    let db = get_or_create_db(db_path)?;

    let selected = super::selector::resolve_entities(&payload, &config, &db);
    let text_entities = if selected.is_empty() {
        db.entities_with(&["text"])?
    } else {
        selected
            .into_iter()
            .filter(|e| db.has_component(e, "text"))
            .collect()
    };

    let mut processed = 0;
    let mut all_quads: Vec<Value> = Vec::new();

    for entity in &text_entities {
        let text_asset = match db.get_component(entity, "text") {
            Ok(a) => a,
            Err(_) => continue,
        };
        let text_data: Value = text_asset
            .entry
            .inline_data
            .unwrap_or_else(|| serde_json::from_slice(&text_asset.data).unwrap_or(json!({})));

        let content = text_data
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if content.is_empty() {
            continue;
        }

        let font_id = text_data
            .get("font")
            .and_then(|v| v.as_str())
            .unwrap_or("default:font");
        let font_size = text_data
            .get("fontSize")
            .and_then(|v| v.as_f64())
            .unwrap_or(16.0) as f32;
        let color = text_data
            .get("color")
            .and_then(|v| v.as_array())
            .map(|a| [fv(a, 0, 1.0), fv(a, 1, 1.0), fv(a, 2, 1.0), fv(a, 3, 1.0)])
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let align = text_data
            .get("align")
            .and_then(|v| v.as_str())
            .unwrap_or("left");
        let max_width = text_data
            .get("maxWidth")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;
        let line_height = text_data
            .get("lineHeight")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.2) as f32;
        let letter_spacing = text_data
            .get("letterSpacing")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;
        let is_sdf = text_data
            .get("sdf")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Load font data from AssetDB
        let font_bytes = match db.get(font_id) {
            Ok(asset) => asset.data,
            Err(_) => {
                // Use embedded minimal font if no font asset
                continue;
            }
        };

        // Build/get atlas
        let atlas = match get_or_build_atlas(font_id, &font_bytes, font_size, is_sdf, content) {
            Ok(a) => a,
            Err(_) => continue,
        };

        // Layout text
        let (glyphs, text_width, text_height) = layout_text(
            content,
            &atlas,
            font_size,
            max_width,
            line_height,
            letter_spacing,
            align,
        );

        // Generate quad data
        let atlas_w = atlas.width as f32;
        let atlas_h = atlas.height as f32;

        let quads: Vec<Value> = glyphs
            .iter()
            .map(|g| {
                let u0 = g.info.atlas_x as f32 / atlas_w;
                let v0 = g.info.atlas_y as f32 / atlas_h;
                let u1 = (g.info.atlas_x + g.info.width) as f32 / atlas_w;
                let v1 = (g.info.atlas_y + g.info.height) as f32 / atlas_h;

                json!({
                    "char": g.ch.to_string(),
                    "x": g.x, "y": g.y,
                    "width": g.info.width, "height": g.info.height,
                    "uv": [u0, v0, u1, v1],
                })
            })
            .collect();

        // Quads are ephemeral — flow through DAG, not persisted.
        // Only the source :text component (prefab data) is stored.
        // The atlas IS persisted since it's a cacheable asset.

        // Store atlas bitmap as binary component on the font entity
        let atlas_id = format!(
            "{}:atlas_{}_{}",
            font_id.split(':').next().unwrap_or("font"),
            font_size as u32,
            if is_sdf { "sdf" } else { "bmp" }
        );
        if !db.has(&atlas_id) {
            let _ = db.put(
                &atlas_id,
                &atlas.bitmap,
                json!({
                    "width": atlas.width,
                    "height": atlas.height,
                    "sdf": is_sdf,
                    "channels": 1,
                }),
            );
        }

        all_quads.push(json!({
            "entity": entity,
            "quads": quads,
            "atlasWidth": atlas.width,
            "atlasHeight": atlas.height,
            "textWidth": text_width,
            "textHeight": text_height,
            "color": color,
            "sdf": is_sdf,
            "font": font_id,
            "outline": text_data.get("outline"),
            "shadow": text_data.get("shadow"),
        }));

        processed += 1;
    }

    let mut out = HashMap::new();
    if !all_quads.is_empty() {
        out.insert(
            "text_quads".to_string(),
            Message::object(EncodableValue::from(json!(all_quads))),
        );
    }
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "textEntitiesProcessed": processed,
        }))),
    );
    Ok(out)
}

fn fv(a: &[Value], idx: usize, default: f64) -> f64 {
    a.get(idx).and_then(|v| v.as_f64()).unwrap_or(default)
}
