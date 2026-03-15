//! Sample format conversion and interleaving utilities.
//!
//! Converts between i16, i32, f32, and f64 sample formats. Handles
//! interleaved ↔ planar channel layout conversion.

/// Convert i16 samples to f32 (range -1.0 to ~1.0).
///
/// When `simd` feature is enabled, dispatches to NEON (aarch64), SSE2 (x86_64).
#[inline]
pub fn i16_to_f32(samples: &[i16], output: &mut [f32]) {
    debug_assert_eq!(samples.len(), output.len());

    #[cfg(all(feature = "simd", target_arch = "aarch64"))]
    {
        unsafe {
            return crate::simd_sample_neon::i16_to_f32_neon(samples, output);
        }
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        unsafe {
            return crate::simd_sample_x86::i16_to_f32_sse2(samples, output);
        }
    }

    #[allow(unreachable_code)]
    i16_to_f32_scalar(samples, output);
}

/// Scalar i16→f32 fallback.
pub(crate) fn i16_to_f32_scalar(samples: &[i16], output: &mut [f32]) {
    for (s, o) in samples.iter().zip(output.iter_mut()) {
        *o = *s as f32 / 32768.0;
    }
}

/// Convert f32 samples to i16 with clamping.
///
/// When `simd` feature is enabled, dispatches to NEON (aarch64), SSE2 (x86_64).
#[inline]
pub fn f32_to_i16(samples: &[f32], output: &mut [i16]) {
    debug_assert_eq!(samples.len(), output.len());

    #[cfg(all(feature = "simd", target_arch = "aarch64"))]
    {
        unsafe {
            return crate::simd_sample_neon::f32_to_i16_neon(samples, output);
        }
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        unsafe {
            return crate::simd_sample_x86::f32_to_i16_sse2(samples, output);
        }
    }

    #[allow(unreachable_code)]
    f32_to_i16_scalar(samples, output);
}

/// Scalar f32→i16 fallback.
pub(crate) fn f32_to_i16_scalar(samples: &[f32], output: &mut [i16]) {
    for (s, o) in samples.iter().zip(output.iter_mut()) {
        let clamped = s.clamp(-1.0, 1.0);
        *o = (clamped * 32767.0) as i16;
    }
}

/// Convert i32 (24-bit in upper bits) to f32.
#[inline]
pub fn i32_to_f32(samples: &[i32], output: &mut [f32]) {
    debug_assert_eq!(samples.len(), output.len());
    for (s, o) in samples.iter().zip(output.iter_mut()) {
        *o = *s as f32 / 2147483648.0;
    }
}

/// Convert f32 to i32 with clamping.
#[inline]
pub fn f32_to_i32(samples: &[f32], output: &mut [i32]) {
    debug_assert_eq!(samples.len(), output.len());
    for (s, o) in samples.iter().zip(output.iter_mut()) {
        let clamped = s.clamp(-1.0, 1.0);
        *o = (clamped * 2147483647.0) as i32;
    }
}

/// Interpret raw bytes as little-endian i16 samples and convert to f32.
pub fn bytes_le_i16_to_f32(bytes: &[u8]) -> Vec<f32> {
    let sample_count = bytes.len() / 2;
    let mut output = Vec::with_capacity(sample_count);
    for chunk in bytes.chunks_exact(2) {
        let s = i16::from_le_bytes([chunk[0], chunk[1]]);
        output.push(s as f32 / 32768.0);
    }
    output
}

/// Convert f32 samples to little-endian i16 bytes.
pub fn f32_to_bytes_le_i16(samples: &[f32]) -> Vec<u8> {
    let mut output = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let i = (clamped * 32767.0) as i16;
        output.extend_from_slice(&i.to_le_bytes());
    }
    output
}

/// Interpret raw bytes as little-endian f32 samples.
pub fn bytes_le_f32_to_f32(bytes: &[u8]) -> Vec<f32> {
    let sample_count = bytes.len() / 4;
    let mut output = Vec::with_capacity(sample_count);
    for chunk in bytes.chunks_exact(4) {
        output.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    output
}

/// Convert f32 samples to little-endian f32 bytes.
pub fn f32_to_bytes_le_f32(samples: &[f32]) -> Vec<u8> {
    let mut output = Vec::with_capacity(samples.len() * 4);
    for &s in samples {
        output.extend_from_slice(&s.to_le_bytes());
    }
    output
}

/// Deinterleave multi-channel audio into separate channel buffers.
///
/// Input: `[L0, R0, L1, R1, ...]` → Output: `[[L0, L1, ...], [R0, R1, ...]]`
pub fn deinterleave(interleaved: &[f32], channels: usize) -> Vec<Vec<f32>> {
    let frames = interleaved.len() / channels;
    let mut output: Vec<Vec<f32>> = (0..channels).map(|_| Vec::with_capacity(frames)).collect();
    for (i, &s) in interleaved.iter().enumerate() {
        output[i % channels].push(s);
    }
    output
}

/// Interleave separate channel buffers into one buffer.
///
/// Input: `[[L0, L1, ...], [R0, R1, ...]]` → Output: `[L0, R0, L1, R1, ...]`
pub fn interleave(channels: &[&[f32]]) -> Vec<f32> {
    if channels.is_empty() {
        return Vec::new();
    }
    let frames = channels[0].len();
    let ch_count = channels.len();
    let mut output = Vec::with_capacity(frames * ch_count);
    for frame in 0..frames {
        for ch in channels {
            output.push(if frame < ch.len() { ch[frame] } else { 0.0 });
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i16_roundtrip() {
        let input: Vec<i16> = vec![0, 16384, -16384, 32767, -32768];
        let mut f32_buf = vec![0.0f32; input.len()];
        i16_to_f32(&input, &mut f32_buf);

        assert!((f32_buf[0]).abs() < 1e-5);
        assert!((f32_buf[1] - 0.5).abs() < 0.001);
        assert!((f32_buf[2] + 0.5).abs() < 0.001);

        let mut back = vec![0i16; input.len()];
        f32_to_i16(&f32_buf, &mut back);
        // Allow ±1 quantization error
        for (a, b) in input.iter().zip(back.iter()) {
            assert!((*a as i32 - *b as i32).abs() <= 1, "{} vs {}", a, b);
        }
    }

    #[test]
    fn test_bytes_le_i16_roundtrip() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let bytes = f32_to_bytes_le_i16(&samples);
        let back = bytes_le_i16_to_f32(&bytes);

        for (a, b) in samples.iter().zip(back.iter()) {
            assert!((a - b).abs() < 0.001, "{} vs {}", a, b);
        }
    }

    #[test]
    fn test_deinterleave_stereo() {
        let interleaved = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let channels = deinterleave(&interleaved, 2);
        assert_eq!(channels[0], vec![1.0, 3.0, 5.0]);
        assert_eq!(channels[1], vec![2.0, 4.0, 6.0]);
    }

    #[test]
    fn test_interleave_stereo() {
        let left = vec![1.0, 3.0, 5.0];
        let right = vec![2.0, 4.0, 6.0];
        let result = interleave(&[&left, &right]);
        assert_eq!(result, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn test_roundtrip_interleave() {
        let original = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let channels = deinterleave(&original, 2);
        let refs: Vec<&[f32]> = channels.iter().map(|c| c.as_slice()).collect();
        let back = interleave(&refs);
        assert_eq!(original, back);
    }
}
