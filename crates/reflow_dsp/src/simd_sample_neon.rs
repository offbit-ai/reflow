//! NEON (aarch64) SIMD implementations for sample format conversion.

#[cfg(target_arch = "aarch64")]
use core::arch::aarch64::*;

/// Convert i16 samples to f32 using NEON — 4 samples per iteration.
///
/// # Safety
/// Caller must ensure aarch64 NEON is available.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub unsafe fn i16_to_f32_neon(samples: &[i16], output: &mut [f32]) {
    debug_assert_eq!(samples.len(), output.len());
    let n = samples.len();
    let chunks = n / 4;
    let remainder = n % 4;

    let scale = vdupq_n_f32(1.0 / 32768.0);

    let mut i = 0;
    for _ in 0..chunks {
        // Load 4 i16 values
        let s16 = vld1_s16(samples.as_ptr().add(i));
        // Widen to i32
        let s32 = vmovl_s16(s16);
        // Convert to f32
        let f = vcvtq_f32_s32(s32);
        // Scale
        let scaled = vmulq_f32(f, scale);
        // Store 4 f32 values
        vst1q_f32(output.as_mut_ptr().add(i), scaled);
        i += 4;
    }

    for j in 0..remainder {
        output[i + j] = samples[i + j] as f32 / 32768.0;
    }
}

/// Convert f32 samples to i16 with clamping using NEON — 4 samples per iteration.
///
/// # Safety
/// Caller must ensure aarch64 NEON is available.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub unsafe fn f32_to_i16_neon(samples: &[f32], output: &mut [i16]) {
    debug_assert_eq!(samples.len(), output.len());
    let n = samples.len();
    let chunks = n / 4;
    let remainder = n % 4;

    let scale = vdupq_n_f32(32767.0);
    let vmin = vdupq_n_f32(-1.0);
    let vmax = vdupq_n_f32(1.0);

    let mut i = 0;
    for _ in 0..chunks {
        let f = vld1q_f32(samples.as_ptr().add(i));
        // Clamp to [-1, 1]
        let clamped = vminq_f32(vmaxq_f32(f, vmin), vmax);
        // Scale to i16 range
        let scaled = vmulq_f32(clamped, scale);
        // Convert to i32 (rounding)
        let i32_val = vcvtnq_s32_f32(scaled);
        // Narrow to i16 (saturating)
        let i16_val = vqmovn_s32(i32_val);
        vst1_s16(output.as_mut_ptr().add(i), i16_val);
        i += 4;
    }

    for j in 0..remainder {
        let clamped = samples[i + j].clamp(-1.0, 1.0);
        output[i + j] = (clamped * 32767.0) as i16;
    }
}

/// Apply window function using NEON — 4 samples per iteration.
///
/// # Safety
/// Caller must ensure aarch64 NEON is available.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub unsafe fn apply_window_neon(samples: &mut [f32], window: &[f32]) {
    debug_assert_eq!(samples.len(), window.len());
    let n = samples.len();
    let chunks = n / 4;
    let remainder = n % 4;

    let mut i = 0;
    for _ in 0..chunks {
        let s = vld1q_f32(samples.as_ptr().add(i));
        let w = vld1q_f32(window.as_ptr().add(i));
        let result = vmulq_f32(s, w);
        vst1q_f32(samples.as_mut_ptr().add(i), result);
        i += 4;
    }

    for j in 0..remainder {
        samples[i + j] *= window[i + j];
    }
}

/// Deinterleave stereo f32 using NEON — 4 frames (8 samples) per iteration.
///
/// # Safety
/// Caller must ensure aarch64 NEON is available.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
#[allow(dead_code)]
pub unsafe fn deinterleave_stereo_neon(interleaved: &[f32], left: &mut [f32], right: &mut [f32]) {
    let frames = left.len();
    debug_assert_eq!(interleaved.len(), frames * 2);
    debug_assert_eq!(left.len(), right.len());

    let chunks = frames / 4;
    let remainder = frames % 4;

    let mut i = 0;
    for _ in 0..chunks {
        // vld2q_f32 loads 8 floats and deinterleaves into 2 registers of 4
        let pair = vld2q_f32(interleaved.as_ptr().add(i * 2));
        vst1q_f32(left.as_mut_ptr().add(i), pair.0);
        vst1q_f32(right.as_mut_ptr().add(i), pair.1);
        i += 4;
    }

    for j in 0..remainder {
        left[i + j] = interleaved[(i + j) * 2];
        right[i + j] = interleaved[(i + j) * 2 + 1];
    }
}

/// Interleave stereo f32 using NEON — 4 frames (8 samples) per iteration.
///
/// # Safety
/// Caller must ensure aarch64 NEON is available.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
#[allow(dead_code)]
pub unsafe fn interleave_stereo_neon(left: &[f32], right: &[f32], output: &mut [f32]) {
    let frames = left.len();
    debug_assert_eq!(left.len(), right.len());
    debug_assert_eq!(output.len(), frames * 2);

    let chunks = frames / 4;
    let remainder = frames % 4;

    let mut i = 0;
    for _ in 0..chunks {
        let l = vld1q_f32(left.as_ptr().add(i));
        let r = vld1q_f32(right.as_ptr().add(i));
        let pair = float32x4x2_t(l, r);
        vst2q_f32(output.as_mut_ptr().add(i * 2), pair);
        i += 4;
    }

    for j in 0..remainder {
        output[(i + j) * 2] = left[i + j];
        output[(i + j) * 2 + 1] = right[i + j];
    }
}

#[cfg(test)]
#[cfg(target_arch = "aarch64")]
mod tests {
    use super::*;

    #[test]
    fn test_neon_i16_to_f32() {
        let input: Vec<i16> = vec![0, 16384, -16384, 32767, -32768, 100, -100, 0, 1000];
        let mut output_neon = vec![0.0f32; input.len()];
        let mut output_scalar = vec![0.0f32; input.len()];

        unsafe {
            i16_to_f32_neon(&input, &mut output_neon);
        }
        crate::sample::i16_to_f32(&input, &mut output_scalar);

        for (i, (n, s)) in output_neon.iter().zip(output_scalar.iter()).enumerate() {
            assert!(
                (n - s).abs() < 1e-6,
                "Sample {}: NEON={} Scalar={}",
                i,
                n,
                s
            );
        }
    }

    #[test]
    fn test_neon_f32_to_i16() {
        let input = vec![0.0f32, 0.5, -0.5, 1.0, -1.0, 0.25, -0.75, 0.0, 0.1];
        let mut output_neon = vec![0i16; input.len()];
        let mut output_scalar = vec![0i16; input.len()];

        unsafe {
            f32_to_i16_neon(&input, &mut output_neon);
        }
        crate::sample::f32_to_i16(&input, &mut output_scalar);

        for (i, (n, s)) in output_neon.iter().zip(output_scalar.iter()).enumerate() {
            assert!(
                (*n as i32 - *s as i32).abs() <= 1,
                "Sample {}: NEON={} Scalar={}",
                i,
                n,
                s
            );
        }
    }

    #[test]
    fn test_neon_apply_window() {
        let mut samples_neon = vec![1.0f32, 2.0, 3.0, 4.0, 5.0];
        let mut samples_scalar = samples_neon.clone();
        let window = vec![0.5f32, 0.8, 1.0, 0.8, 0.5];

        unsafe {
            apply_window_neon(&mut samples_neon, &window);
        }
        crate::window::apply(&mut samples_scalar, &window);

        for (n, s) in samples_neon.iter().zip(samples_scalar.iter()) {
            assert!((n - s).abs() < 1e-6, "NEON={} Scalar={}", n, s);
        }
    }

    #[test]
    fn test_neon_deinterleave_stereo() {
        let interleaved = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let frames = interleaved.len() / 2;
        let mut left = vec![0.0f32; frames];
        let mut right = vec![0.0f32; frames];

        unsafe {
            deinterleave_stereo_neon(&interleaved, &mut left, &mut right);
        }

        assert_eq!(left, vec![1.0, 3.0, 5.0, 7.0, 9.0]);
        assert_eq!(right, vec![2.0, 4.0, 6.0, 8.0, 10.0]);
    }

    #[test]
    fn test_neon_interleave_stereo() {
        let left = vec![1.0f32, 3.0, 5.0, 7.0, 9.0];
        let right = vec![2.0f32, 4.0, 6.0, 8.0, 10.0];
        let mut output = vec![0.0f32; 10];

        unsafe {
            interleave_stereo_neon(&left, &right, &mut output);
        }

        assert_eq!(
            output,
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
        );
    }

    #[test]
    fn test_neon_roundtrip_interleave() {
        let original = vec![
            1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
        ];
        let frames = original.len() / 2;
        let mut left = vec![0.0f32; frames];
        let mut right = vec![0.0f32; frames];
        let mut back = vec![0.0f32; original.len()];

        unsafe {
            deinterleave_stereo_neon(&original, &mut left, &mut right);
            interleave_stereo_neon(&left, &right, &mut back);
        }

        assert_eq!(original, back);
    }
}
