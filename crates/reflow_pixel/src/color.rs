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
        assert!((g as i32 - 76).abs() <= 1, "Red gray should be ~76, got {}", g);
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
                r, g, b, hsv, r2, g2, b2
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
        // All channels should be equal (gray)
        assert_eq!(row[0], row[1]);
        assert_eq!(row[1], row[2]);
    }
}
