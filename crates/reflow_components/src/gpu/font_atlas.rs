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
    // For SDF: rasterize at 4x resolution for high-quality distance fields,
    // then generate SDF from the hi-res bitmap. Metrics are scaled back to
    // the requested font_size so layout math stays correct.
    let sdf_padding = if is_sdf { 8u32 } else { 1 };
    let render_scale: f32 = if is_sdf { 4.0 } else { 1.0 };
    let render_size = font_size * render_scale;
    let mut glyph_bitmaps: Vec<(char, Vec<u8>, usize, usize, fontdue::Metrics)> = Vec::new();

    for &ch in &unique_chars {
        let (metrics, bitmap) = font.rasterize(ch, render_size);
        if is_sdf {
            let sdf = generate_sdf(&bitmap, metrics.width, metrics.height, sdf_padding);
            let sdf_w = metrics.width + sdf_padding as usize * 2;
            let sdf_h = metrics.height + sdf_padding as usize * 2;
            // Keep metrics at render_scale; the renderer divides by font_scale
            // (estimated from glyph height) to map to display size consistently.
            let sdf_metrics = fontdue::Metrics {
                width: sdf_w,
                height: sdf_h,
                ..metrics
            };
            glyph_bitmaps.push((ch, sdf, sdf_w, sdf_h, sdf_metrics));
        } else {
            let w = metrics.width;
            let h = metrics.height;
            glyph_bitmaps.push((ch, bitmap, w, h, metrics));
        }
    }

    // Pack glyphs into atlas (row packing)
    let max_glyph_h = glyph_bitmaps
        .iter()
        .map(|(_, _, _, h, _)| *h)
        .max()
        .unwrap_or(0);
    let total_area: usize = glyph_bitmaps
        .iter()
        .map(|(_, _, w, h, _)| (w + 2) * (h + 2))
        .sum();
    let atlas_width = ((total_area as f64).sqrt() * 1.2) as u32;
    let atlas_width = atlas_width.max(512).min(4096).next_power_of_two();

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
        .max(max_glyph_h as u32 + 2)
        .min(8192);

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

/// Generate an SDF from a grayscale glyph bitmap.
///
/// Uses dual 8SSEDT (8-point Sequential Signed Euclidean Distance Transform):
/// one pass for outside-boundary distances, one for inside-boundary distances,
/// combined into a signed distance field. Binary threshold at 127 with exact
/// Euclidean propagation via (dx,dy) offset tracking.
fn generate_sdf(bitmap: &[u8], w: usize, h: usize, padding: u32) -> Vec<u8> {
    let pw = w + padding as usize * 2;
    let ph = h + padding as usize * 2;
    let spread = padding as f32;
    let pad = padding as usize;
    let n = pw * ph;
    let inf = (pw + ph) as f32;

    // Sample source bitmap with padding (outside = 0)
    let sample = |gx: i32, gy: i32| -> bool {
        if gx >= 0 && gx < w as i32 && gy >= 0 && gy < h as i32 {
            bitmap[gy as usize * w + gx as usize] > 127
        } else {
            false
        }
    };

    // Build padded inside/outside grid
    let mut inside = vec![false; n];
    for sy in 0..ph {
        for sx in 0..pw {
            inside[sy * pw + sx] = sample(sx as i32 - pad as i32, sy as i32 - pad as i32);
        }
    }

    // Compute distance from boundary for outside pixels
    let dist_out = edt_8ssedt(&inside, pw, ph, false);
    // Compute distance from boundary for inside pixels
    let dist_in = edt_8ssedt(&inside, pw, ph, true);

    // Combine: signed distance = inside - outside, normalize to 0..255
    let mut sdf = vec![0u8; n];
    for i in 0..n {
        let signed = dist_in[i] - dist_out[i];
        let normalized = (signed / spread * 127.0 + 128.0).clamp(0.0, 255.0) as u8;
        sdf[i] = normalized;
    }
    sdf
}

/// 8SSEDT: compute Euclidean distance from the boundary of `mask`.
/// If `invert` is false, computes distance for pixels where mask=false (outside).
/// If `invert` is true, computes distance for pixels where mask=true (inside).
/// Returns distance field (0.0 at boundary, increasing away from it).
fn edt_8ssedt(mask: &[bool], w: usize, h: usize, invert: bool) -> Vec<f32> {
    let n = w * h;
    let big = (w + h) as i32;
    let mut dist = vec![0.0f32; n];
    let mut ox = vec![0i32; n];
    let mut oy = vec![0i32; n];

    // Seed: boundary pixels get dist=0. Interior pixels of the target
    // region get dist=INF (will be propagated). Non-target pixels get 0.
    for i in 0..n {
        let is_target = mask[i] ^ invert; // outside if !invert, inside if invert
        if is_target {
            dist[i] = 0.0;
            ox[i] = 0;
            oy[i] = 0;
        } else {
            // Check if this is a boundary pixel (neighbor is target)
            let x = (i % w) as i32;
            let y = (i / w) as i32;
            let has_target_neighbor =
                [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)]
                    .iter()
                    .any(|&(dx, dy)| {
                        let nx = x + dx;
                        let ny = y + dy;
                        if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                            mask[ny as usize * w + nx as usize] ^ invert
                        } else {
                            false
                        }
                    });
            if has_target_neighbor {
                dist[i] = 0.5; // half-pixel from boundary
                ox[i] = 0;
                oy[i] = 0;
            } else {
                dist[i] = big as f32;
                ox[i] = big;
                oy[i] = big;
            }
        }
    }

    // Forward pass: top-left to bottom-right
    let fwd_offsets: [(i32, i32); 4] = [(-1, 0), (-1, -1), (0, -1), (1, -1)];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let i = y as usize * w + x as usize;
            for &(ddx, ddy) in &fwd_offsets {
                let nx = x + ddx;
                let ny = y + ddy;
                if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                    let ni = ny as usize * w + nx as usize;
                    let ndx = ox[ni] - ddx;
                    let ndy = oy[ni] - ddy;
                    let nd = ((ndx * ndx + ndy * ndy) as f32).sqrt();
                    if nd < dist[i] {
                        dist[i] = nd;
                        ox[i] = ndx;
                        oy[i] = ndy;
                    }
                }
            }
        }
    }

    // Backward pass: bottom-right to top-left
    let bwd_offsets: [(i32, i32); 4] = [(1, 0), (1, 1), (0, 1), (-1, 1)];
    for y in (0..h as i32).rev() {
        for x in (0..w as i32).rev() {
            let i = y as usize * w + x as usize;
            for &(ddx, ddy) in &bwd_offsets {
                let nx = x + ddx;
                let ny = y + ddy;
                if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                    let ni = ny as usize * w + nx as usize;
                    let ndx = ox[ni] - ddx;
                    let ndy = oy[ni] - ddy;
                    let nd = ((ndx * ndx + ndy * ndy) as f32).sqrt();
                    if nd < dist[i] {
                        dist[i] = nd;
                        ox[i] = ndx;
                        oy[i] = ndy;
                    }
                }
            }
        }
    }

    dist
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
