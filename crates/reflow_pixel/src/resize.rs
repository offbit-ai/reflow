//! Bilinear interpolation for image resizing.
//!
//! Used by ImageResize, VideoScale, ThumbnailActor, and PiPCompose actors.
//! Operates on row buffers — for streaming, the caller maintains a 2-row
//! window and calls [`interpolate_row`] for each output row.

use crate::format::PixelFormat;

/// Resize an entire image buffer using bilinear interpolation.
///
/// `src`: source pixel buffer (contiguous, row-major)
/// `src_width`, `src_height`: source dimensions
/// `dst_width`, `dst_height`: target dimensions
/// `format`: pixel format (determines bytes per pixel)
///
/// Returns a new buffer of size `dst_width * dst_height * bpp`.
pub fn resize(
    src: &[u8],
    src_width: usize,
    src_height: usize,
    dst_width: usize,
    dst_height: usize,
    format: PixelFormat,
) -> Vec<u8> {
    let bpp = format.bpp();
    let mut dst = vec![0u8; dst_width * dst_height * bpp];

    let x_ratio = if dst_width > 1 {
        (src_width - 1) as f64 / (dst_width - 1) as f64
    } else {
        0.0
    };
    let y_ratio = if dst_height > 1 {
        (src_height - 1) as f64 / (dst_height - 1) as f64
    } else {
        0.0
    };

    for dy in 0..dst_height {
        let sy = y_ratio * dy as f64;
        let y0 = sy.floor() as usize;
        let y1 = (y0 + 1).min(src_height - 1);
        let y_frac = (sy - y0 as f64) as f32;

        for dx in 0..dst_width {
            let sx = x_ratio * dx as f64;
            let x0 = sx.floor() as usize;
            let x1 = (x0 + 1).min(src_width - 1);
            let x_frac = (sx - x0 as f64) as f32;

            let dst_offset = (dy * dst_width + dx) * bpp;

            for c in 0..bpp {
                let tl = src[(y0 * src_width + x0) * bpp + c] as f32;
                let tr = src[(y0 * src_width + x1) * bpp + c] as f32;
                let bl = src[(y1 * src_width + x0) * bpp + c] as f32;
                let br = src[(y1 * src_width + x1) * bpp + c] as f32;

                let top = tl + (tr - tl) * x_frac;
                let bot = bl + (br - bl) * x_frac;
                let val = top + (bot - top) * y_frac;

                dst[dst_offset + c] = val.round().clamp(0.0, 255.0) as u8;
            }
        }
    }

    dst
}

/// Bilinear interpolation between two source rows to produce one output row.
///
/// This is the streaming-friendly API: the caller maintains a sliding window
/// of two adjacent source rows and calls this for each output row.
///
/// - `row_top`, `row_bot`: two adjacent source rows (same width, same format)
/// - `y_frac`: vertical interpolation factor (0.0 = top row, 1.0 = bottom row)
/// - `src_width`: source row width in pixels
/// - `dst_width`: output row width in pixels
/// - `bpp`: bytes per pixel
/// - `output`: output row buffer (length = dst_width * bpp)
pub fn interpolate_row(
    row_top: &[u8],
    row_bot: &[u8],
    y_frac: f32,
    src_width: usize,
    dst_width: usize,
    bpp: usize,
    output: &mut [u8],
) {
    debug_assert_eq!(row_top.len(), src_width * bpp);
    debug_assert_eq!(row_bot.len(), src_width * bpp);
    debug_assert_eq!(output.len(), dst_width * bpp);

    let x_ratio = if dst_width > 1 {
        (src_width - 1) as f64 / (dst_width - 1) as f64
    } else {
        0.0
    };

    for dx in 0..dst_width {
        let sx = x_ratio * dx as f64;
        let x0 = sx.floor() as usize;
        let x1 = (x0 + 1).min(src_width - 1);
        let x_frac = (sx - x0 as f64) as f32;

        let dst_offset = dx * bpp;

        for c in 0..bpp {
            let tl = row_top[x0 * bpp + c] as f32;
            let tr = row_top[x1 * bpp + c] as f32;
            let bl = row_bot[x0 * bpp + c] as f32;
            let br = row_bot[x1 * bpp + c] as f32;

            let top = tl + (tr - tl) * x_frac;
            let bot = bl + (br - bl) * x_frac;
            let val = top + (bot - top) * y_frac;

            output[dst_offset + c] = val.round().clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resize_identity() {
        // Resize 2x2 → 2x2 should be identity
        let src = vec![10, 20, 30, 40, 50, 60, 70, 80]; // 2x2 GrayAlpha
        let dst = resize(&src, 2, 2, 2, 2, PixelFormat::GrayAlpha8);
        assert_eq!(src, dst);
    }

    #[test]
    fn test_resize_upscale_gray() {
        // 2x1 gray [0, 255] → 3x1 should give [0, 128, 255]
        let src = vec![0u8, 255];
        let dst = resize(&src, 2, 1, 3, 1, PixelFormat::Gray8);
        assert_eq!(dst.len(), 3);
        assert_eq!(dst[0], 0);
        assert!((dst[1] as i32 - 128).abs() <= 1);
        assert_eq!(dst[2], 255);
    }

    #[test]
    fn test_resize_downscale() {
        // 4x1 gray [0, 85, 170, 255] → 2x1 should give [0, 255] (endpoints)
        let src = vec![0, 85, 170, 255];
        let dst = resize(&src, 4, 1, 2, 1, PixelFormat::Gray8);
        assert_eq!(dst[0], 0);
        assert_eq!(dst[1], 255);
    }

    #[test]
    fn test_interpolate_row_no_vertical() {
        // y_frac=0 → pure top row, just horizontal resizing
        let top = vec![0u8, 255]; // 2 pixels gray
        let bot = vec![128, 128];
        let mut out = vec![0u8; 3];
        interpolate_row(&top, &bot, 0.0, 2, 3, 1, &mut out);
        assert_eq!(out[0], 0);
        assert!((out[1] as i32 - 128).abs() <= 1);
        assert_eq!(out[2], 255);
    }

    #[test]
    fn test_interpolate_row_pure_vertical() {
        // Same width, just vertical interpolation at y_frac=0.5
        let top = vec![0u8; 4]; // 1 RGBA pixel, black
        let bot = vec![200, 200, 200, 255]; // gray
        let mut out = vec![0u8; 4];
        interpolate_row(&top, &bot, 0.5, 1, 1, 4, &mut out);
        assert_eq!(out[0], 100);
        assert_eq!(out[1], 100);
        assert_eq!(out[2], 100);
        assert!((out[3] as i32 - 128).abs() <= 1);
    }
}
