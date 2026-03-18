//! Gaussian blur kernel — separable two-pass (horizontal + vertical).
//!
//! Operates on full RGBA buffers. For streaming compatibility, callers
//! must collect the full frame first, blur, then re-emit.

/// Apply gaussian blur to an RGBA buffer in-place.
///
/// `width`/`height`: image dimensions. `radius`: blur radius in pixels.
/// Uses separable two-pass approach: horizontal then vertical.
pub fn gaussian_blur(buf: &mut [u8], width: usize, height: usize, radius: usize) {
    if radius == 0 || width == 0 || height == 0 {
        return;
    }

    let kernel = build_kernel(radius);
    let mut temp = vec![0u8; buf.len()];

    // Horizontal pass: buf → temp
    blur_horizontal(buf, &mut temp, width, height, &kernel, radius);

    // Vertical pass: temp → buf
    blur_vertical(&temp, buf, width, height, &kernel, radius);
}

/// Build 1D gaussian kernel (one side + center).
/// Returns weights for [0..=radius], normalized to sum to 1.0.
fn build_kernel(radius: usize) -> Vec<f32> {
    let sigma = (radius as f32) / 3.0;
    let sigma2 = 2.0 * sigma * sigma;
    let mut kernel = Vec::with_capacity(radius + 1);
    let mut sum = 0.0f32;

    for i in 0..=radius {
        let w = (-(i as f32 * i as f32) / sigma2).exp();
        kernel.push(w);
        sum += if i == 0 { w } else { 2.0 * w };
    }

    // Normalize
    for w in &mut kernel {
        *w /= sum;
    }
    kernel
}

fn blur_horizontal(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    kernel: &[f32],
    _radius: usize,
) {
    for y in 0..height {
        let row_off = y * width * 4;
        for x in 0..width {
            let mut r = 0.0f32;
            let mut g = 0.0f32;
            let mut b = 0.0f32;
            let mut a = 0.0f32;

            for (k, &w) in kernel.iter().enumerate() {
                // Center
                if k == 0 {
                    let off = row_off + x * 4;
                    r += src[off] as f32 * w;
                    g += src[off + 1] as f32 * w;
                    b += src[off + 2] as f32 * w;
                    a += src[off + 3] as f32 * w;
                } else {
                    // Left
                    let lx = x.saturating_sub(k);
                    let off = row_off + lx * 4;
                    r += src[off] as f32 * w;
                    g += src[off + 1] as f32 * w;
                    b += src[off + 2] as f32 * w;
                    a += src[off + 3] as f32 * w;

                    // Right
                    let rx = (x + k).min(width - 1);
                    let off = row_off + rx * 4;
                    r += src[off] as f32 * w;
                    g += src[off + 1] as f32 * w;
                    b += src[off + 2] as f32 * w;
                    a += src[off + 3] as f32 * w;
                }
            }

            let off = row_off + x * 4;
            dst[off] = r.round().clamp(0.0, 255.0) as u8;
            dst[off + 1] = g.round().clamp(0.0, 255.0) as u8;
            dst[off + 2] = b.round().clamp(0.0, 255.0) as u8;
            dst[off + 3] = a.round().clamp(0.0, 255.0) as u8;
        }
    }
}

fn blur_vertical(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    kernel: &[f32],
    _radius: usize,
) {
    for y in 0..height {
        for x in 0..width {
            let mut r = 0.0f32;
            let mut g = 0.0f32;
            let mut b = 0.0f32;
            let mut a = 0.0f32;

            for (k, &w) in kernel.iter().enumerate() {
                if k == 0 {
                    let off = (y * width + x) * 4;
                    r += src[off] as f32 * w;
                    g += src[off + 1] as f32 * w;
                    b += src[off + 2] as f32 * w;
                    a += src[off + 3] as f32 * w;
                } else {
                    let ty = y.saturating_sub(k);
                    let off = (ty * width + x) * 4;
                    r += src[off] as f32 * w;
                    g += src[off + 1] as f32 * w;
                    b += src[off + 2] as f32 * w;
                    a += src[off + 3] as f32 * w;

                    let by = (y + k).min(height - 1);
                    let off = (by * width + x) * 4;
                    r += src[off] as f32 * w;
                    g += src[off + 1] as f32 * w;
                    b += src[off + 2] as f32 * w;
                    a += src[off + 3] as f32 * w;
                }
            }

            let off = (y * width + x) * 4;
            dst[off] = r.round().clamp(0.0, 255.0) as u8;
            dst[off + 1] = g.round().clamp(0.0, 255.0) as u8;
            dst[off + 2] = b.round().clamp(0.0, 255.0) as u8;
            dst[off + 3] = a.round().clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_preserves_solid_color() {
        // A solid-color image should be unchanged after blur
        let mut buf = vec![128u8; 10 * 10 * 4];
        gaussian_blur(&mut buf, 10, 10, 2);
        assert_eq!(buf[0], 128);
        assert_eq!(buf[40], 128);
    }

    #[test]
    fn blur_smooths_edge() {
        // Single white pixel in black field
        let mut buf = vec![0u8; 10 * 10 * 4];
        let center = (5 * 10 + 5) * 4;
        buf[center] = 255;
        buf[center + 1] = 255;
        buf[center + 2] = 255;
        buf[center + 3] = 255;

        gaussian_blur(&mut buf, 10, 10, 2);

        // Center should be dimmer (spread out)
        assert!(buf[center] < 255);
        assert!(buf[center] > 0);

        // Neighbors should have picked up some white
        let neighbor = (5 * 10 + 6) * 4;
        assert!(buf[neighbor] > 0);
    }

    #[test]
    fn kernel_sums_to_one() {
        let kernel = build_kernel(5);
        let sum: f32 = kernel[0] + 2.0 * kernel[1..].iter().sum::<f32>();
        assert!((sum - 1.0).abs() < 0.01, "Kernel sum = {}", sum);
    }
}
