//! SSE2 (x86_64) SIMD implementations for sample conversion and windowing.

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

/// Convert i16 samples to f32 using SSE2 — 4 samples per iteration.
///
/// # Safety
/// Caller must ensure SSE2 is available (always true on x86_64).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
pub unsafe fn i16_to_f32_sse2(samples: &[i16], output: &mut [f32]) {
    debug_assert_eq!(samples.len(), output.len());
    let n = samples.len();
    let chunks = n / 4;
    let remainder = n % 4;

    let scale = _mm_set1_ps(1.0 / 32768.0);

    let mut i = 0;
    for _ in 0..chunks {
        // Load 4 i16 values (8 bytes) — zero extend lower 8 bytes of 128-bit load
        let s16 = _mm_loadl_epi64(samples.as_ptr().add(i) as *const __m128i);
        // Sign-extend i16 to i32 using unpack with sign
        let s32 = _mm_srai_epi32(_mm_unpacklo_epi16(s16, s16), 16);
        // Convert i32 to f32
        let f = _mm_cvtepi32_ps(s32);
        // Scale
        let scaled = _mm_mul_ps(f, scale);
        _mm_storeu_ps(output.as_mut_ptr().add(i), scaled);
        i += 4;
    }

    for j in 0..remainder {
        output[i + j] = samples[i + j] as f32 / 32768.0;
    }
}

/// Convert f32 samples to i16 with clamping using SSE2 — 4 samples per iteration.
///
/// # Safety
/// Caller must ensure SSE2 is available.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
pub unsafe fn f32_to_i16_sse2(samples: &[f32], output: &mut [i16]) {
    debug_assert_eq!(samples.len(), output.len());
    let n = samples.len();
    let chunks = n / 4;
    let remainder = n % 4;

    let scale = _mm_set1_ps(32767.0);
    let vmin = _mm_set1_ps(-1.0);
    let vmax = _mm_set1_ps(1.0);

    let mut i = 0;
    for _ in 0..chunks {
        let f = _mm_loadu_ps(samples.as_ptr().add(i));
        let clamped = _mm_min_ps(_mm_max_ps(f, vmin), vmax);
        let scaled = _mm_mul_ps(clamped, scale);
        // Convert to i32 with rounding
        let i32_val = _mm_cvtps_epi32(scaled);
        // Saturating pack i32 → i16 (packs 8 i32s, we only use low 4)
        let i16_val = _mm_packs_epi32(i32_val, _mm_setzero_si128());
        // Store low 8 bytes (4 i16 values)
        _mm_storel_epi64(output.as_mut_ptr().add(i) as *mut __m128i, i16_val);
        i += 4;
    }

    for j in 0..remainder {
        let clamped = samples[i + j].clamp(-1.0, 1.0);
        output[i + j] = (clamped * 32767.0) as i16;
    }
}

/// Apply window function using SSE2 — 4 samples per iteration.
///
/// # Safety
/// Caller must ensure SSE2 is available.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
pub unsafe fn apply_window_sse2(samples: &mut [f32], window: &[f32]) {
    debug_assert_eq!(samples.len(), window.len());
    let n = samples.len();
    let chunks = n / 4;
    let remainder = n % 4;

    let mut i = 0;
    for _ in 0..chunks {
        let s = _mm_loadu_ps(samples.as_ptr().add(i));
        let w = _mm_loadu_ps(window.as_ptr().add(i));
        let result = _mm_mul_ps(s, w);
        _mm_storeu_ps(samples.as_mut_ptr().add(i), result);
        i += 4;
    }

    for j in 0..remainder {
        samples[i + j] *= window[i + j];
    }
}

#[cfg(test)]
#[cfg(target_arch = "x86_64")]
mod tests {
    use super::*;

    #[test]
    fn test_sse2_i16_to_f32() {
        let input: Vec<i16> = vec![0, 16384, -16384, 32767, -32768, 100, -100, 0, 1000];
        let mut output_simd = vec![0.0f32; input.len()];
        let mut output_scalar = vec![0.0f32; input.len()];

        unsafe {
            i16_to_f32_sse2(&input, &mut output_simd);
        }
        crate::sample::i16_to_f32(&input, &mut output_scalar);

        for (i, (s, sc)) in output_simd.iter().zip(output_scalar.iter()).enumerate() {
            assert!(
                (s - sc).abs() < 1e-5,
                "Sample {}: SSE2={} Scalar={}",
                i,
                s,
                sc
            );
        }
    }

    #[test]
    fn test_sse2_f32_to_i16() {
        let input = vec![0.0f32, 0.5, -0.5, 1.0, -1.0, 0.25, -0.75, 0.0, 0.1];
        let mut output_simd = vec![0i16; input.len()];
        let mut output_scalar = vec![0i16; input.len()];

        unsafe {
            f32_to_i16_sse2(&input, &mut output_simd);
        }
        crate::sample::f32_to_i16(&input, &mut output_scalar);

        for (i, (s, sc)) in output_simd.iter().zip(output_scalar.iter()).enumerate() {
            assert!(
                (*s as i32 - *sc as i32).abs() <= 1,
                "Sample {}: SSE2={} Scalar={}",
                i,
                s,
                sc
            );
        }
    }

    #[test]
    fn test_sse2_apply_window() {
        let mut samples_simd = vec![1.0f32, 2.0, 3.0, 4.0, 5.0];
        let mut samples_scalar = samples_simd.clone();
        let window = vec![0.5f32, 0.8, 1.0, 0.8, 0.5];

        unsafe {
            apply_window_sse2(&mut samples_simd, &window);
        }
        crate::window::apply(&mut samples_scalar, &window);

        for (s, sc) in samples_simd.iter().zip(samples_scalar.iter()) {
            assert!((s - sc).abs() < 1e-6, "SSE2={} Scalar={}", s, sc);
        }
    }
}
