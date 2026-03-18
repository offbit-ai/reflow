//! SSE2 (x86_64) SIMD implementations for color operations.
//!
//! Processes 4 RGBA pixels at a time using 128-bit SSE2 registers.
//! SSE2 is guaranteed on all x86_64 targets — no runtime detection needed.

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

/// Convert RGBA row to grayscale using SSE2 — 4 pixels per iteration.
///
/// Uses i16 widening multiply with BT.601 coefficients.
///
/// # Safety
/// Caller must ensure SSE2 is available (always true on x86_64).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
pub unsafe fn row_rgba_to_gray_sse2(input: &[u8], output: &mut [u8]) {
    let pixel_count = output.len();
    let chunks = pixel_count / 4;
    let remainder = pixel_count % 4;

    let zero = _mm_setzero_si128();
    // BT.601 coefficients as i16 (we'll use low 4 lanes)
    let coeff_r = _mm_set1_epi16(77);
    let coeff_g = _mm_set1_epi16(150);
    let coeff_b = _mm_set1_epi16(29);

    let mut i = 0usize;
    for _ in 0..chunks {
        let base = i * 4;
        // Load 16 bytes (4 RGBA pixels)
        let rgba = _mm_loadu_si128(input.as_ptr().add(base) as *const __m128i);

        // Extract R, G, B channels by shuffling + zero-extending to i16
        // RGBA layout: [R0,G0,B0,A0, R1,G1,B1,A1, R2,G2,B2,A2, R3,G3,B3,A3]
        // Shuffle to get R bytes at positions 0,2,4,6 with zeros at 1,3,5,7
        let r_shuf = _mm_set_epi8(
            -1, -1, -1, -1, -1, -1, -1, -1, // upper 8 bytes = 0
            -1, 12, -1, 8, -1, 4, -1, 0, // R0, R1, R2, R3 zero-extended
        );
        let g_shuf = _mm_set_epi8(-1, -1, -1, -1, -1, -1, -1, -1, -1, 13, -1, 9, -1, 5, -1, 1);
        let b_shuf = _mm_set_epi8(-1, -1, -1, -1, -1, -1, -1, -1, -1, 14, -1, 10, -1, 6, -1, 2);

        // _mm_shuffle_epi8 requires SSSE3, use unpack instead for SSE2
        // Extract bytes manually via shifts and masks
        let mask_byte0 = _mm_set1_epi32(0x000000FF_u32 as i32);

        // R: byte 0 of each 4-byte group
        let r_32 = _mm_and_si128(rgba, mask_byte0);
        // G: byte 1 of each 4-byte group
        let g_32 = _mm_and_si128(_mm_srli_epi32(rgba, 8), mask_byte0);
        // B: byte 2 of each 4-byte group
        let b_32 = _mm_and_si128(_mm_srli_epi32(rgba, 16), mask_byte0);

        // Pack i32x4 → i16x4 (low half): we need to convert to i16 for mullo
        // Since values are 0-255, they fit in i16 as-is via packing
        let r_16 = _mm_packs_epi32(r_32, zero);
        let g_16 = _mm_packs_epi32(g_32, zero);
        let b_16 = _mm_packs_epi32(b_32, zero);

        // Multiply (low 16-bit multiply)
        let r_prod = _mm_mullo_epi16(r_16, coeff_r);
        let g_prod = _mm_mullo_epi16(g_16, coeff_g);
        let b_prod = _mm_mullo_epi16(b_16, coeff_b);

        // Sum and shift right by 8
        let sum = _mm_add_epi16(_mm_add_epi16(r_prod, g_prod), b_prod);
        let shifted = _mm_srli_epi16(sum, 8);

        // Extract 4 results from lanes 0-3
        output[i] = _mm_extract_epi16(shifted, 0) as u8;
        output[i + 1] = _mm_extract_epi16(shifted, 1) as u8;
        output[i + 2] = _mm_extract_epi16(shifted, 2) as u8;
        output[i + 3] = _mm_extract_epi16(shifted, 3) as u8;

        i += 4;
    }

    // Scalar remainder
    for j in 0..remainder {
        let idx = (i + j) * 4;
        output[i + j] = super::color::rgba_to_gray(input[idx], input[idx + 1], input[idx + 2]);
    }
}

/// Apply brightness to RGBA row using SSE — 4 pixels per iteration.
///
/// # Safety
/// Caller must ensure SSE2 is available.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
pub unsafe fn row_brightness_sse2(row: &mut [u8], factor: f32) {
    let pixel_count = row.len() / 4;
    let chunks = pixel_count / 4;
    let remainder = pixel_count % 4;

    let vfactor = _mm_set1_ps(factor);
    let vzero = _mm_setzero_ps();
    let vmax = _mm_set1_ps(255.0);

    let mut i = 0usize;
    for _ in 0..chunks {
        let base = i * 4;

        // Process R, G, B of 4 pixels
        for ch in 0..3usize {
            let vals = _mm_set_ps(
                row[base + 12 + ch] as f32,
                row[base + 8 + ch] as f32,
                row[base + 4 + ch] as f32,
                row[base + ch] as f32,
            );
            let scaled = _mm_mul_ps(vals, vfactor);
            let clamped = _mm_min_ps(_mm_max_ps(scaled, vzero), vmax);

            // Extract and store
            let mut result = [0.0f32; 4];
            _mm_storeu_ps(result.as_mut_ptr(), clamped);
            row[base + ch] = result[0] as u8;
            row[base + 4 + ch] = result[1] as u8;
            row[base + 8 + ch] = result[2] as u8;
            row[base + 12 + ch] = result[3] as u8;
        }

        i += 4;
    }

    for j in 0..remainder {
        let base = (i + j) * 4;
        row[base] = ((row[base] as f32 * factor).clamp(0.0, 255.0)) as u8;
        row[base + 1] = ((row[base + 1] as f32 * factor).clamp(0.0, 255.0)) as u8;
        row[base + 2] = ((row[base + 2] as f32 * factor).clamp(0.0, 255.0)) as u8;
    }
}

#[cfg(test)]
#[cfg(target_arch = "x86_64")]
mod tests {
    use super::*;

    #[test]
    fn test_sse2_rgba_to_gray() {
        let mut input = vec![0u8; 16 * 4];
        let mut output_simd = vec![0u8; 16];
        let mut output_scalar = vec![0u8; 16];

        for i in 0..16 {
            input[i * 4] = (i * 16) as u8;
            input[i * 4 + 1] = (255 - i * 16) as u8;
            input[i * 4 + 2] = (i * 8) as u8;
            input[i * 4 + 3] = 255;
        }

        unsafe {
            row_rgba_to_gray_sse2(&input, &mut output_simd);
        }
        super::super::color::row_rgba_to_gray_scalar(&input, &mut output_scalar);

        for i in 0..16 {
            assert!(
                (output_simd[i] as i32 - output_scalar[i] as i32).abs() <= 1,
                "Pixel {}: SSE2={} Scalar={}",
                i,
                output_simd[i],
                output_scalar[i]
            );
        }
    }

    #[test]
    fn test_sse2_brightness() {
        let mut row_simd = vec![
            100u8, 150, 200, 255, 50, 100, 200, 255, 0, 0, 0, 255, 255, 255, 255, 255,
        ];
        let mut row_scalar = row_simd.clone();

        unsafe {
            row_brightness_sse2(&mut row_simd, 1.5);
        }
        super::super::color::row_brightness(&mut row_scalar, 1.5);

        for i in 0..row_simd.len() {
            assert!(
                (row_simd[i] as i32 - row_scalar[i] as i32).abs() <= 1,
                "Byte {}: SSE2={} Scalar={}",
                i,
                row_simd[i],
                row_scalar[i]
            );
        }
    }
}
