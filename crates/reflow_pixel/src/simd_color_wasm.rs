//! WASM SIMD (simd128) implementations for color operations.
//!
//! Processes 4 RGBA pixels at a time using 128-bit WASM SIMD.
//! Requires compilation with `-C target-feature=+simd128`.

#[cfg(target_arch = "wasm32")]
use core::arch::wasm32::*;

/// Convert RGBA row to grayscale using WASM SIMD — 4 pixels per iteration.
///
/// Uses i16x8 widening to avoid overflow: max is 77*255 + 150*255 + 29*255 = 65280.
///
/// # Safety
/// Caller must ensure wasm32 simd128 is available.
#[cfg(target_arch = "wasm32")]
#[target_feature(enable = "simd128")]
pub unsafe fn row_rgba_to_gray_wasm(input: &[u8], output: &mut [u8]) {
    let pixel_count = output.len();
    let chunks = pixel_count / 4;
    let remainder = pixel_count % 4;

    // BT.601 coefficients as i16
    let coeff_r = i16x8_splat(77);
    let coeff_g = i16x8_splat(150);
    let coeff_b = i16x8_splat(29);
    let zero = i8x16_splat(0);

    let mut i = 0usize;
    for _ in 0..chunks {
        let base = i * 4;
        // Load 16 bytes = 4 RGBA pixels
        let rgba = v128_load(input.as_ptr().add(base) as *const v128);

        // Extract R, G, B channels using shuffle
        // RGBA layout: [R0,G0,B0,A0, R1,G1,B1,A1, R2,G2,B2,A2, R3,G3,B3,A3]
        // We need: R = [R0,0, R1,0, R2,0, R3,0, 0,0,0,0,0,0,0,0] as i16x8
        let r_bytes = i8x16_shuffle::<0, 16, 4, 16, 8, 16, 12, 16, 16, 16, 16, 16, 16, 16, 16, 16>(
            rgba, zero,
        );
        let g_bytes = i8x16_shuffle::<1, 16, 5, 16, 9, 16, 13, 16, 16, 16, 16, 16, 16, 16, 16, 16>(
            rgba, zero,
        );
        let b_bytes = i8x16_shuffle::<2, 16, 6, 16, 10, 16, 14, 16, 16, 16, 16, 16, 16, 16, 16, 16>(
            rgba, zero,
        );

        // Now r_bytes/g_bytes/b_bytes are i16x8 with values in low 4 lanes (zero-extended)
        // Multiply and sum
        let r_prod = i16x8_mul(r_bytes, coeff_r);
        let g_prod = i16x8_mul(g_bytes, coeff_g);
        let b_prod = i16x8_mul(b_bytes, coeff_b);
        let sum = i16x8_add(i16x8_add(r_prod, g_prod), b_prod);

        // Shift right by 8
        let shifted = u16x8_shr(sum, 8);

        // Extract the 4 results from lanes 0,1,2,3
        output[i] = u16x8_extract_lane::<0>(shifted) as u8;
        output[i + 1] = u16x8_extract_lane::<1>(shifted) as u8;
        output[i + 2] = u16x8_extract_lane::<2>(shifted) as u8;
        output[i + 3] = u16x8_extract_lane::<3>(shifted) as u8;

        i += 4;
    }

    // Scalar remainder
    for j in 0..remainder {
        let idx = (i + j) * 4;
        output[i + j] = super::color::rgba_to_gray(input[idx], input[idx + 1], input[idx + 2]);
    }
}

/// Apply brightness to RGBA row using WASM SIMD — 4 pixels per iteration.
///
/// # Safety
/// Caller must ensure wasm32 simd128 is available.
#[cfg(target_arch = "wasm32")]
#[target_feature(enable = "simd128")]
pub unsafe fn row_brightness_wasm(row: &mut [u8], factor: f32) {
    let pixel_count = row.len() / 4;
    let chunks = pixel_count / 4;
    let remainder = pixel_count % 4;

    let vfactor = f32x4_splat(factor);
    let vzero = f32x4_splat(0.0);
    let vmax = f32x4_splat(255.0);

    let mut i = 0usize;
    for _ in 0..chunks {
        let base = i * 4;

        // Process R channel of 4 pixels
        let r = f32x4(
            row[base] as f32,
            row[base + 4] as f32,
            row[base + 8] as f32,
            row[base + 12] as f32,
        );
        let r = f32x4_max(vzero, f32x4_min(vmax, f32x4_mul(r, vfactor)));

        let g = f32x4(
            row[base + 1] as f32,
            row[base + 5] as f32,
            row[base + 9] as f32,
            row[base + 13] as f32,
        );
        let g = f32x4_max(vzero, f32x4_min(vmax, f32x4_mul(g, vfactor)));

        let b = f32x4(
            row[base + 2] as f32,
            row[base + 6] as f32,
            row[base + 10] as f32,
            row[base + 14] as f32,
        );
        let b = f32x4_max(vzero, f32x4_min(vmax, f32x4_mul(b, vfactor)));

        // Store back
        row[base] = f32x4_extract_lane::<0>(r) as u8;
        row[base + 4] = f32x4_extract_lane::<1>(r) as u8;
        row[base + 8] = f32x4_extract_lane::<2>(r) as u8;
        row[base + 12] = f32x4_extract_lane::<3>(r) as u8;

        row[base + 1] = f32x4_extract_lane::<0>(g) as u8;
        row[base + 5] = f32x4_extract_lane::<1>(g) as u8;
        row[base + 9] = f32x4_extract_lane::<2>(g) as u8;
        row[base + 13] = f32x4_extract_lane::<3>(g) as u8;

        row[base + 2] = f32x4_extract_lane::<0>(b) as u8;
        row[base + 6] = f32x4_extract_lane::<1>(b) as u8;
        row[base + 10] = f32x4_extract_lane::<2>(b) as u8;
        row[base + 14] = f32x4_extract_lane::<3>(b) as u8;

        i += 4;
    }

    // Scalar remainder
    for j in 0..remainder {
        let base = (i + j) * 4;
        row[base] = ((row[base] as f32 * factor).clamp(0.0, 255.0)) as u8;
        row[base + 1] = ((row[base + 1] as f32 * factor).clamp(0.0, 255.0)) as u8;
        row[base + 2] = ((row[base + 2] as f32 * factor).clamp(0.0, 255.0)) as u8;
    }
}
