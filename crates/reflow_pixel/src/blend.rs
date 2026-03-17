//! Alpha blending and compositing operations.
//!
//! Used by ImageBlend, ImageCompose, PiPCompose, ChromaKey, and VideoOverlay
//! actors. All operations work on row buffers for streaming compatibility.

/// Blend mode for compositing two layers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlendMode {
    /// Standard alpha blending (Porter-Duff "over").
    Normal,
    /// Multiply: result = base * overlay.
    Multiply,
    /// Screen: result = 1 - (1-base)(1-overlay).
    Screen,
    /// Overlay: Multiply where base < 0.5, Screen where base >= 0.5.
    Overlay,
    /// Additive: result = base + overlay (clamped).
    Add,
    /// Soft light: gentle dodge/burn.
    SoftLight,
    /// Hard light: overlay with swapped roles.
    HardLight,
    /// Difference: |base - overlay|.
    Difference,
    /// Exclusion: base + overlay - 2*base*overlay.
    Exclusion,
    /// Color dodge: base / (1 - overlay).
    ColorDodge,
    /// Color burn: 1 - (1-base) / overlay.
    ColorBurn,
    /// Darken: min(base, overlay).
    Darken,
    /// Lighten: max(base, overlay).
    Lighten,
    /// Subtract: base - overlay (clamped).
    Subtract,
    /// Divide: base / overlay.
    Divide,
}

impl BlendMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().replace(['-', '_', ' '], "").as_str() {
            "normal" => Self::Normal,
            "multiply" => Self::Multiply,
            "screen" => Self::Screen,
            "overlay" => Self::Overlay,
            "add" | "lineardodge" => Self::Add,
            "softlight" => Self::SoftLight,
            "hardlight" => Self::HardLight,
            "difference" => Self::Difference,
            "exclusion" => Self::Exclusion,
            "colordodge" => Self::ColorDodge,
            "colorburn" => Self::ColorBurn,
            "darken" => Self::Darken,
            "lighten" => Self::Lighten,
            "subtract" | "linearburn" => Self::Subtract,
            "divide" => Self::Divide,
            _ => Self::Normal,
        }
    }
}

/// Alpha-composite a single foreground pixel over a background pixel (RGBA).
///
/// Uses Porter-Duff "over" operator. Modifies `bg` in place.
#[inline]
pub fn alpha_over(bg: &mut [u8; 4], fg: &[u8; 4]) {
    let fg_a = fg[3] as f32 / 255.0;
    let bg_a = bg[3] as f32 / 255.0;
    let out_a = fg_a + bg_a * (1.0 - fg_a);

    if out_a < 1e-6 {
        *bg = [0, 0, 0, 0];
        return;
    }

    for i in 0..3 {
        let fg_c = fg[i] as f32 / 255.0;
        let bg_c = bg[i] as f32 / 255.0;
        let out_c = (fg_c * fg_a + bg_c * bg_a * (1.0 - fg_a)) / out_a;
        bg[i] = (out_c * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    bg[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// Blend two RGBA rows using the specified blend mode. Result written to `base`.
///
/// `base` and `overlay` must have the same length (width * 4).
/// `opacity`: 0.0–1.0, applied to overlay before blending.
pub fn blend_rows(base: &mut [u8], overlay: &[u8], mode: BlendMode, opacity: f32) {
    debug_assert_eq!(base.len(), overlay.len());
    debug_assert_eq!(base.len() % 4, 0);

    for (bg_px, fg_px) in base.chunks_exact_mut(4).zip(overlay.chunks_exact(4)) {
        let fg_a = (fg_px[3] as f32 / 255.0) * opacity;

        let blended: [u8; 3] = match mode {
            BlendMode::Normal => [fg_px[0], fg_px[1], fg_px[2]],
            BlendMode::Multiply => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    let b = bg_px[i] as f32 / 255.0;
                    let f = fg_px[i] as f32 / 255.0;
                    out[i] = ((b * f) * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                out
            }
            BlendMode::Screen => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    let b = bg_px[i] as f32 / 255.0;
                    let f = fg_px[i] as f32 / 255.0;
                    out[i] = ((1.0 - (1.0 - b) * (1.0 - f)) * 255.0)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                }
                out
            }
            BlendMode::Overlay => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    let b = bg_px[i] as f32 / 255.0;
                    let f = fg_px[i] as f32 / 255.0;
                    let v = if b < 0.5 {
                        2.0 * b * f
                    } else {
                        1.0 - 2.0 * (1.0 - b) * (1.0 - f)
                    };
                    out[i] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                out
            }
            BlendMode::Add => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    out[i] = (bg_px[i] as u16 + fg_px[i] as u16).min(255) as u8;
                }
                out
            }
            BlendMode::SoftLight => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    let b = bg_px[i] as f32 / 255.0;
                    let f = fg_px[i] as f32 / 255.0;
                    let v = if f <= 0.5 {
                        b - (1.0 - 2.0 * f) * b * (1.0 - b)
                    } else {
                        let d = if b <= 0.25 {
                            ((16.0 * b - 12.0) * b + 4.0) * b
                        } else {
                            b.sqrt()
                        };
                        b + (2.0 * f - 1.0) * (d - b)
                    };
                    out[i] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                out
            }
            BlendMode::HardLight => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    let b = bg_px[i] as f32 / 255.0;
                    let f = fg_px[i] as f32 / 255.0;
                    let v = if f < 0.5 {
                        2.0 * b * f
                    } else {
                        1.0 - 2.0 * (1.0 - b) * (1.0 - f)
                    };
                    out[i] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                out
            }
            BlendMode::Difference => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    out[i] = (bg_px[i] as i16 - fg_px[i] as i16).unsigned_abs() as u8;
                }
                out
            }
            BlendMode::Exclusion => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    let b = bg_px[i] as f32 / 255.0;
                    let f = fg_px[i] as f32 / 255.0;
                    let v = b + f - 2.0 * b * f;
                    out[i] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                out
            }
            BlendMode::ColorDodge => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    let b = bg_px[i] as f32 / 255.0;
                    let f = fg_px[i] as f32 / 255.0;
                    let v = if f >= 1.0 { 1.0 } else { (b / (1.0 - f)).min(1.0) };
                    out[i] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                out
            }
            BlendMode::ColorBurn => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    let b = bg_px[i] as f32 / 255.0;
                    let f = fg_px[i] as f32 / 255.0;
                    let v = if f <= 0.0 { 0.0 } else { 1.0 - ((1.0 - b) / f).min(1.0) };
                    out[i] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                out
            }
            BlendMode::Darken => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    out[i] = bg_px[i].min(fg_px[i]);
                }
                out
            }
            BlendMode::Lighten => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    out[i] = bg_px[i].max(fg_px[i]);
                }
                out
            }
            BlendMode::Subtract => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    out[i] = bg_px[i].saturating_sub(fg_px[i]);
                }
                out
            }
            BlendMode::Divide => {
                let mut out = [0u8; 3];
                for i in 0..3 {
                    let b = bg_px[i] as f32 / 255.0;
                    let f = fg_px[i] as f32 / 255.0;
                    let v = if f <= 0.0 { 1.0 } else { (b / f).min(1.0) };
                    out[i] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                out
            }
        };

        // Lerp between base and blended result using effective alpha
        let bg_a = bg_px[3] as f32 / 255.0;
        let out_a = fg_a + bg_a * (1.0 - fg_a);

        if out_a < 1e-6 {
            bg_px[0] = 0;
            bg_px[1] = 0;
            bg_px[2] = 0;
            bg_px[3] = 0;
        } else {
            for i in 0..3 {
                let bg_c = bg_px[i] as f32;
                let fg_c = blended[i] as f32;
                bg_px[i] = (bg_c * (1.0 - fg_a) + fg_c * fg_a)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
            bg_px[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Composite `foreground` onto `background` at pixel offset `(x, y)`.
///
/// Both buffers are RGBA, with `fg_width`/`bg_width` specifying row widths.
/// Rows that fall outside the background bounds are clipped.
pub fn composite_at(
    bg: &mut [u8],
    bg_width: usize,
    fg: &[u8],
    fg_width: usize,
    fg_height: usize,
    x: usize,
    y: usize,
) {
    let bg_stride = bg_width * 4;
    let fg_stride = fg_width * 4;
    let bg_height = bg.len() / bg_stride;

    for row in 0..fg_height {
        let bg_y = y + row;
        if bg_y >= bg_height {
            break;
        }

        let fg_row_start = row * fg_stride;
        let fg_row_end = fg_row_start + fg_stride;
        if fg_row_end > fg.len() {
            break;
        }

        let bg_row_start = bg_y * bg_stride;

        for col in 0..fg_width {
            let bg_x = x + col;
            if bg_x >= bg_width {
                break;
            }

            let fg_offset = fg_row_start + col * 4;
            let bg_offset = bg_row_start + bg_x * 4;

            let mut bg_px = [bg[bg_offset], bg[bg_offset + 1], bg[bg_offset + 2], bg[bg_offset + 3]];
            let fg_px = [fg[fg_offset], fg[fg_offset + 1], fg[fg_offset + 2], fg[fg_offset + 3]];

            alpha_over(&mut bg_px, &fg_px);

            bg[bg_offset..bg_offset + 4].copy_from_slice(&bg_px);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alpha_over_opaque_fg() {
        let mut bg = [100, 100, 100, 255];
        let fg = [200, 50, 50, 255];
        alpha_over(&mut bg, &fg);
        assert_eq!(bg, [200, 50, 50, 255]); // fully opaque fg replaces bg
    }

    #[test]
    fn test_alpha_over_transparent_fg() {
        let mut bg = [100, 100, 100, 255];
        let fg = [200, 50, 50, 0];
        alpha_over(&mut bg, &fg);
        assert_eq!(bg, [100, 100, 100, 255]); // transparent fg, bg unchanged
    }

    #[test]
    fn test_alpha_over_half_transparent() {
        let mut bg = [0, 0, 0, 255];
        let fg = [255, 255, 255, 128]; // ~50% white
        alpha_over(&mut bg, &fg);
        // Result should be roughly 50% gray
        assert!((bg[0] as i32 - 128).abs() <= 2);
    }

    #[test]
    fn test_blend_rows_normal() {
        let mut base = [100, 100, 100, 255, 50, 50, 50, 255];
        let overlay = [200, 200, 200, 128, 0, 0, 0, 128];
        blend_rows(&mut base, &overlay, BlendMode::Normal, 1.0);

        // First pixel: ~50% blend of 100 and 200 = ~150
        assert!((base[0] as i32 - 150).abs() <= 2);
    }

    #[test]
    fn test_blend_rows_multiply() {
        let mut base = [200, 200, 200, 255];
        let overlay = [128, 128, 128, 255]; // 50% gray, fully opaque
        blend_rows(&mut base, &overlay, BlendMode::Multiply, 1.0);
        // 200/255 * 128/255 * 255 ≈ 100
        assert!((base[0] as i32 - 100).abs() <= 2, "got {}", base[0]);
    }

    #[test]
    fn test_blend_opacity_zero() {
        let mut base = [100, 100, 100, 255];
        let overlay = [200, 200, 200, 255];
        blend_rows(&mut base, &overlay, BlendMode::Normal, 0.0);
        assert_eq!(base[0], 100); // opacity 0 = no change
    }

    #[test]
    fn test_composite_at_clip() {
        // 2x2 bg, 2x2 fg placed at (1,1) — only top-left pixel of fg lands on bg
        let mut bg = vec![0u8; 2 * 2 * 4]; // 2x2 black
        let fg = vec![255u8; 2 * 2 * 4]; // 2x2 white opaque
        composite_at(&mut bg, 2, &fg, 2, 2, 1, 1);

        // Only pixel (1,1) should be white
        let offset = (1 * 2 + 1) * 4;
        assert_eq!(bg[offset], 255);
        // Pixel (0,0) should still be black
        assert_eq!(bg[0], 0);
    }
}
