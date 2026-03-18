//! Color space conversions.
//!
//! Used by ChromaKey (HSV hue matching), ColorCorrection, ColorAdjust,
//! and GrayscaleFilter actors. All operations work on individual pixels
//! or row buffers for streaming compatibility.

/// Convert RGBA pixel to grayscale using ITU-R BT.601 luma coefficients.
///
/// Returns a single byte (0-255).
#[inline]
pub fn rgba_to_gray(r: u8, g: u8, b: u8) -> u8 {
    // 0.299R + 0.587G + 0.114B — fixed-point for speed
    ((77 * r as u32 + 150 * g as u32 + 29 * b as u32) >> 8) as u8
}

/// Convert an RGBA row to a Gray8 row in-place into `output`.
///
/// `input` must have length `width * 4`, `output` must have length `width`.
///
/// When the `simd` feature is enabled, uses platform-specific SIMD:
/// - aarch64: NEON `vmull_u8` — 8 pixels per iteration
/// - wasm32: `i16x8` — 4 pixels per iteration
pub fn row_rgba_to_gray(input: &[u8], output: &mut [u8]) {
    debug_assert_eq!(input.len(), output.len() * 4);

    #[cfg(all(feature = "simd", target_arch = "aarch64"))]
    {
        // SAFETY: NEON is always available on aarch64
        unsafe {
            return crate::simd_color_neon::row_rgba_to_gray_neon(input, output);
        }
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        // SAFETY: SSE2 is always available on x86_64
        unsafe {
            return crate::simd_color_x86::row_rgba_to_gray_sse2(input, output);
        }
    }

    #[cfg(all(feature = "simd", target_arch = "wasm32", target_feature = "simd128"))]
    {
        // SAFETY: simd128 feature is enabled at compile time
        unsafe {
            return crate::simd_color_wasm::row_rgba_to_gray_wasm(input, output);
        }
    }

    // Scalar fallback
    #[allow(unreachable_code)]
    {
        row_rgba_to_gray_scalar(input, output);
    }
}

/// Scalar implementation of RGBA→Gray, always available for testing.
pub(crate) fn row_rgba_to_gray_scalar(input: &[u8], output: &mut [u8]) {
    for (pixel, out) in input.chunks_exact(4).zip(output.iter_mut()) {
        *out = rgba_to_gray(pixel[0], pixel[1], pixel[2]);
    }
}

/// Convert an RGB row to a Gray8 row.
pub fn row_rgb_to_gray(input: &[u8], output: &mut [u8]) {
    debug_assert_eq!(input.len(), output.len() * 3);
    for (pixel, out) in input.chunks_exact(3).zip(output.iter_mut()) {
        *out = rgba_to_gray(pixel[0], pixel[1], pixel[2]);
    }
}

/// HSV representation (all components 0.0–1.0, hue 0.0–360.0).
#[derive(Debug, Clone, Copy)]
pub struct Hsv {
    /// Hue in degrees (0.0–360.0).
    pub h: f32,
    /// Saturation (0.0–1.0).
    pub s: f32,
    /// Value / brightness (0.0–1.0).
    pub v: f32,
}

/// Convert RGB (0-255 each) to HSV.
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> Hsv {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;

    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;

    let h = if delta < 1e-6 {
        0.0
    } else if (max - rf).abs() < 1e-6 {
        60.0 * (((gf - bf) / delta) % 6.0)
    } else if (max - gf).abs() < 1e-6 {
        60.0 * ((bf - rf) / delta + 2.0)
    } else {
        60.0 * ((rf - gf) / delta + 4.0)
    };

    let h = if h < 0.0 { h + 360.0 } else { h };

    let s = if max < 1e-6 { 0.0 } else { delta / max };

    Hsv { h, s, v: max }
}

/// Convert HSV to RGB (0-255 each).
pub fn hsv_to_rgb(hsv: Hsv) -> (u8, u8, u8) {
    let Hsv { h, s, v } = hsv;
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    let to_u8 = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (to_u8(r1), to_u8(g1), to_u8(b1))
}

/// Apply brightness adjustment to an RGBA row in-place.
///
/// `factor`: 1.0 = no change, >1.0 = brighter, <1.0 = darker.
///
/// When the `simd` feature is enabled on wasm32 with simd128, uses
/// `f32x4` vectorized multiply+clamp.
pub fn row_brightness(row: &mut [u8], factor: f32) {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        unsafe {
            return crate::simd_color_x86::row_brightness_sse2(row, factor);
        }
    }

    #[cfg(all(feature = "simd", target_arch = "wasm32", target_feature = "simd128"))]
    {
        unsafe {
            return crate::simd_color_wasm::row_brightness_wasm(row, factor);
        }
    }

    // Scalar (also used by NEON — LLVM auto-vectorizes this well at opt-level 2)
    #[allow(unreachable_code)]
    for pixel in row.chunks_exact_mut(4) {
        pixel[0] = ((pixel[0] as f32 * factor).round().clamp(0.0, 255.0)) as u8;
        pixel[1] = ((pixel[1] as f32 * factor).round().clamp(0.0, 255.0)) as u8;
        pixel[2] = ((pixel[2] as f32 * factor).round().clamp(0.0, 255.0)) as u8;
    }
}

/// Apply contrast adjustment to an RGBA row in-place.
///
/// `factor`: 1.0 = no change, >1.0 = more contrast, <1.0 = less contrast.
pub fn row_contrast(row: &mut [u8], factor: f32) {
    for pixel in row.chunks_exact_mut(4) {
        for c in &mut pixel[..3] {
            let v = ((*c as f32 / 255.0 - 0.5) * factor + 0.5) * 255.0;
            *c = v.round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Apply saturation adjustment to an RGBA row in-place.
///
/// `factor`: 1.0 = no change, 0.0 = grayscale, >1.0 = more saturated.
pub fn row_saturation(row: &mut [u8], factor: f32) {
    for pixel in row.chunks_exact_mut(4) {
        let gray = rgba_to_gray(pixel[0], pixel[1], pixel[2]) as f32;
        for c in &mut pixel[..3] {
            let v = gray + (*c as f32 - gray) * factor;
            *c = v.round().clamp(0.0, 255.0) as u8;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HSL color space
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy)]
pub struct Hsl {
    pub h: f32, // 0..360
    pub s: f32, // 0..1
    pub l: f32, // 0..1
}

pub fn rgb_to_hsl(r: u8, g: u8, b: u8) -> Hsl {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;
    let delta = max - min;

    if delta < 1e-6 {
        return Hsl { h: 0.0, s: 0.0, l };
    }

    let s = if l > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let h = if (max - rf).abs() < 1e-6 {
        ((gf - bf) / delta) % 6.0
    } else if (max - gf).abs() < 1e-6 {
        (bf - rf) / delta + 2.0
    } else {
        (rf - gf) / delta + 4.0
    } * 60.0;

    Hsl {
        h: if h < 0.0 { h + 360.0 } else { h },
        s,
        l,
    }
}

pub fn hsl_to_rgb(hsl: Hsl) -> (u8, u8, u8) {
    let Hsl { h, s, l } = hsl;
    if s < 1e-6 {
        let v = (l * 255.0).round().clamp(0.0, 255.0) as u8;
        return (v, v, v);
    }
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let to_u8 = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (to_u8(r1), to_u8(g1), to_u8(b1))
}

// ═══════════════════════════════════════════════════════════════════════════
// OKLAB color space — perceptually uniform, ideal for interpolation
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy)]
pub struct Oklab {
    pub l: f32, // lightness 0..1
    pub a: f32, // green-red axis ~-0.4..0.4
    pub b: f32, // blue-yellow axis ~-0.4..0.4
}

pub fn rgb_to_oklab(r: u8, g: u8, b: u8) -> Oklab {
    // sRGB → linear
    let rl = srgb_to_linear(r as f32 / 255.0);
    let gl = srgb_to_linear(g as f32 / 255.0);
    let bl = srgb_to_linear(b as f32 / 255.0);

    // Linear RGB → LMS
    let l = 0.4122215_f32 * rl + 0.5363325_f32 * gl + 0.0514460_f32 * bl;
    let m = 0.2119035_f32 * rl + 0.6806995_f32 * gl + 0.107_397_f32 * bl;
    let s = 0.0883025_f32 * rl + 0.2817188_f32 * gl + 0.6299787_f32 * bl;

    // LMS → LMS (cube root)
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();

    // LMS → Oklab
    Oklab {
        l: 0.2104543_f32 * l_ + 0.7936178_f32 * m_ - 0.0040720_f32 * s_,
        a: 1.9779985_f32 * l_ - 2.4285922_f32 * m_ + 0.4505937_f32 * s_,
        b: 0.0259040_f32 * l_ + 0.7827718_f32 * m_ - 0.8086758_f32 * s_,
    }
}

pub fn oklab_to_rgb(lab: Oklab) -> (u8, u8, u8) {
    // Oklab → LMS (cube root)
    let l_ = lab.l + 0.3963378_f32 * lab.a + 0.2158038_f32 * lab.b;
    let m_ = lab.l - 0.1055613_f32 * lab.a - 0.0638542_f32 * lab.b;
    let s_ = lab.l - 0.0894842_f32 * lab.a - 1.2914855_f32 * lab.b;

    // LMS (cube root) → LMS
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    // LMS → linear RGB
    let rl = 4.0767417_f32 * l - 3.3077116_f32 * m + 0.2309699_f32 * s;
    let gl = -1.268_438_f32 * l + 2.6097574_f32 * m - 0.3413194_f32 * s;
    let bl = -0.0041961_f32 * l - 0.7034186_f32 * m + 1.7076147_f32 * s;

    // linear → sRGB
    let to_u8 = |v: f32| (linear_to_srgb(v) * 255.0).round().clamp(0.0, 255.0) as u8;
    (to_u8(rl), to_u8(gl), to_u8(bl))
}

/// Interpolate two RGB colors in OKLAB space (perceptually uniform).
pub fn lerp_oklab(r1: u8, g1: u8, b1: u8, r2: u8, g2: u8, b2: u8, t: f32) -> (u8, u8, u8) {
    let a = rgb_to_oklab(r1, g1, b1);
    let b = rgb_to_oklab(r2, g2, b2);
    oklab_to_rgb(Oklab {
        l: a.l + (b.l - a.l) * t,
        a: a.a + (b.a - a.a) * t,
        b: a.b + (b.b - a.b) * t,
    })
}

fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Apply hue shift to an RGBA row in-place.
pub fn row_hue_shift(row: &mut [u8], degrees: f32) {
    for pixel in row.chunks_exact_mut(4) {
        let mut hsl = rgb_to_hsl(pixel[0], pixel[1], pixel[2]);
        hsl.h = (hsl.h + degrees) % 360.0;
        if hsl.h < 0.0 {
            hsl.h += 360.0;
        }
        let (r, g, b) = hsl_to_rgb(hsl);
        pixel[0] = r;
        pixel[1] = g;
        pixel[2] = b;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gray_pure_white() {
        assert_eq!(rgba_to_gray(255, 255, 255), 255);
    }

    #[test]
    fn test_gray_pure_black() {
        assert_eq!(rgba_to_gray(0, 0, 0), 0);
    }

    #[test]
    fn test_gray_red() {
        // 0.299 * 255 ≈ 76
        let g = rgba_to_gray(255, 0, 0);
        assert!(
            (g as i32 - 76).abs() <= 1,
            "Red gray should be ~76, got {}",
            g
        );
    }

    #[test]
    fn test_row_rgba_to_gray() {
        let input = [255, 0, 0, 255, 0, 255, 0, 255]; // red, green
        let mut output = [0u8; 2];
        row_rgba_to_gray(&input, &mut output);
        assert!((output[0] as i32 - 76).abs() <= 1); // red
        assert!((output[1] as i32 - 150).abs() <= 1); // green ≈ 0.587*255
    }

    #[test]
    fn test_hsv_roundtrip() {
        for &(r, g, b) in &[
            (255u8, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (128, 64, 32),
            (0, 0, 0),
            (255, 255, 255),
        ] {
            let hsv = rgb_to_hsv(r, g, b);
            let (r2, g2, b2) = hsv_to_rgb(hsv);
            assert!(
                (r as i32 - r2 as i32).abs() <= 1
                    && (g as i32 - g2 as i32).abs() <= 1
                    && (b as i32 - b2 as i32).abs() <= 1,
                "Roundtrip failed for ({},{},{}) → {:?} → ({},{},{})",
                r,
                g,
                b,
                hsv,
                r2,
                g2,
                b2
            );
        }
    }

    #[test]
    fn test_brightness() {
        let mut row = [100, 100, 100, 255];
        row_brightness(&mut row, 2.0);
        assert_eq!(row[0], 200);
        assert_eq!(row[3], 255); // alpha unchanged
    }

    #[test]
    fn test_brightness_clamps() {
        let mut row = [200, 200, 200, 255];
        row_brightness(&mut row, 2.0);
        assert_eq!(row[0], 255); // clamped
    }

    #[test]
    fn test_saturation_zero_is_gray() {
        let mut row = [255, 0, 0, 255];
        row_saturation(&mut row, 0.0);
        assert_eq!(row[0], row[1]);
        assert_eq!(row[1], row[2]);
    }

    #[test]
    fn test_hsl_roundtrip() {
        for &(r, g, b) in &[(255u8, 0, 0), (0, 255, 0), (0, 0, 255), (128, 64, 32)] {
            let hsl = rgb_to_hsl(r, g, b);
            let (r2, g2, b2) = hsl_to_rgb(hsl);
            assert!(
                (r as i32 - r2 as i32).abs() <= 1
                    && (g as i32 - g2 as i32).abs() <= 1
                    && (b as i32 - b2 as i32).abs() <= 1,
                "HSL roundtrip failed for ({},{},{}) → ({},{},{})",
                r,
                g,
                b,
                r2,
                g2,
                b2
            );
        }
    }

    #[test]
    fn test_oklab_roundtrip() {
        for &(r, g, b) in &[
            (255u8, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (128, 128, 128),
            (255, 255, 0),
        ] {
            let lab = rgb_to_oklab(r, g, b);
            let (r2, g2, b2) = oklab_to_rgb(lab);
            assert!(
                (r as i32 - r2 as i32).abs() <= 2
                    && (g as i32 - g2 as i32).abs() <= 2
                    && (b as i32 - b2 as i32).abs() <= 2,
                "OKLAB roundtrip failed for ({},{},{}) → ({},{},{})",
                r,
                g,
                b,
                r2,
                g2,
                b2
            );
        }
    }

    #[test]
    fn test_oklab_lerp_midpoint() {
        let (r, g, b) = lerp_oklab(0, 0, 0, 255, 255, 255, 0.5);
        // OKLAB perceptual midpoint is lighter than linear midpoint (128)
        // Should be roughly equal across channels (gray)
        assert!(
            (r as i32 - g as i32).abs() <= 2,
            "r={} g={} should be close",
            r,
            g
        );
        assert!(
            (g as i32 - b as i32).abs() <= 2,
            "g={} b={} should be close",
            g,
            b
        );
        assert!(
            r > 50 && r < 250,
            "OKLAB midpoint r={} should be reasonable",
            r
        );
    }

    #[test]
    fn test_hue_shift() {
        let mut row = [255, 0, 0, 255]; // red
        row_hue_shift(&mut row, 120.0); // shift to green
                                        // Should be roughly green
        assert!(
            row[1] > row[0],
            "Hue shift 120° from red should increase green"
        );
    }
}
