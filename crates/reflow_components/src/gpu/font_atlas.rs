//! Shared font atlas utilities — SDF glyph atlas generation from TTF/OTF.
//!
//! Used by both the GPU 2D renderer (PRIM_GLYPH) and the ECS text systems.
//! Pure Rust via `fontdue`, wasm-safe.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

pub struct FontAtlas {
    /// Single-channel bitmap: grayscale or SDF (1 byte per pixel)
    pub bitmap: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub glyphs: HashMap<char, GlyphInfo>,
    pub is_sdf: bool,
}

#[derive(Clone, Debug)]
pub struct GlyphInfo {
    pub atlas_x: u32,
    pub atlas_y: u32,
    pub width: u32,
    pub height: u32,
    pub advance: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
}

/// GPU-ready atlas data for passing to the renderer.
pub struct GlyphAtlasGpu {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

// ═══════════════════════════════════════════════════════════════════════════
// Global atlas cache — shared across ticks and actors
// ═══════════════════════════════════════════════════════════════════════════

static ATLAS_CACHE: OnceLock<RwLock<HashMap<String, Arc<FontAtlas>>>> = OnceLock::new();

fn atlas_cache() -> &'static RwLock<HashMap<String, Arc<FontAtlas>>> {
    ATLAS_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Atlas building
// ═══════════════════════════════════════════════════════════════════════════

/// Build or retrieve a cached SDF glyph atlas for the given font.
///
/// `font_id` is used as part of the cache key (e.g. `"roboto"`).
/// `font_data` is raw TTF/OTF bytes.
/// `font_size` determines rasterization size (larger = more detail in SDF).
/// `chars` is an optional extra character set to include beyond ASCII printable.
pub fn get_or_build_atlas(
    font_id: &str,
    font_data: &[u8],
    font_size: f32,
    is_sdf: bool,
    chars: &str,
) -> anyhow::Result<Arc<FontAtlas>> {
    let cache_key = format!(
        "{}:{}:{}",
        font_id,
        font_size as u32,
        if is_sdf { "sdf" } else { "bmp" }
    );

    // Check cache first
    if let Ok(cache) = atlas_cache().read() {
        if let Some(atlas) = cache.get(&cache_key) {
            return Ok(Arc::clone(atlas));
        }
    }

    // Parse font
    let font = fontdue::Font::from_bytes(font_data, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("Failed to parse font: {}", e))?;

    // Collect unique chars (always include ASCII printable)
    let mut unique_chars: Vec<char> = Vec::new();
    for c in 32u8..=126 {
        unique_chars.push(c as char);
    }
    for ch in chars.chars() {
        if !unique_chars.contains(&ch) {
            unique_chars.push(ch);
        }
    }

    // Rasterize each glyph
    let sdf_padding = if is_sdf { 16u32 } else { 1 };
    let mut glyph_bitmaps: Vec<(char, Vec<u8>, usize, usize, fontdue::Metrics)> = Vec::new();

    for &ch in &unique_chars {
        let (metrics, bitmap) = font.rasterize(ch, font_size);
        if is_sdf {
            let sdf = generate_sdf(&bitmap, metrics.width, metrics.height, sdf_padding);
            let sdf_w = metrics.width + sdf_padding as usize * 2;
            let sdf_h = metrics.height + sdf_padding as usize * 2;
            glyph_bitmaps.push((ch, sdf, sdf_w, sdf_h, metrics));
        } else {
            let w = metrics.width;
            let h = metrics.height;
            glyph_bitmaps.push((ch, bitmap, w, h, metrics));
        }
    }

    // Pack glyphs into atlas (row packing)
    let max_glyph_h = glyph_bitmaps.iter().map(|(_, _, _, h, _)| *h).max().unwrap_or(0);
    let total_width_est: usize = glyph_bitmaps.iter().map(|(_, _, w, _, _)| w + 2).sum();
    let atlas_width = ((total_width_est as f64).sqrt() * 1.5) as u32;
    let atlas_width = atlas_width.max(256).next_power_of_two();

    let mut cursor_x = 1u32;
    let mut cursor_y = 1u32;
    let mut row_height = 0u32;
    let mut glyphs = HashMap::new();

    for (ch, _, gw, gh, metrics) in &glyph_bitmaps {
        let gw = *gw as u32;
        let gh = *gh as u32;

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

    let atlas_height = (cursor_y + row_height + 1)
        .next_power_of_two()
        .max(max_glyph_h as u32 + 2);

    // Blit glyphs into atlas bitmap
    let mut bitmap = vec![0u8; (atlas_width * atlas_height) as usize];
    for (ch, glyph_bmp, gw, _, _) in &glyph_bitmaps {
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

    if let Ok(mut cache) = atlas_cache().write() {
        cache.insert(cache_key, Arc::clone(&atlas));
    }

    Ok(atlas)
}

/// Generate an SDF from a binary glyph bitmap.
/// Brute-force distance computation (acceptable for glyph-sized inputs).
fn generate_sdf(bitmap: &[u8], w: usize, h: usize, padding: u32) -> Vec<u8> {
    let pw = w + padding as usize * 2;
    let ph = h + padding as usize * 2;
    let spread = padding as f32;
    let mut sdf = vec![0u8; pw * ph];

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
            let normalized = (signed / spread * 127.0 + 128.0).clamp(0.0, 255.0) as u8;
            sdf[sy * pw + sx] = normalized;
        }
    }
    sdf
}

/// Convert an `Arc<FontAtlas>` to GPU-ready data.
impl FontAtlas {
    pub fn to_gpu(&self) -> GlyphAtlasGpu {
        GlyphAtlasGpu {
            data: self.bitmap.clone(),
            width: self.width,
            height: self.height,
        }
    }
}
