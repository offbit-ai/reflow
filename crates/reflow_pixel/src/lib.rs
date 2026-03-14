//! Pure-Rust pixel operations for Reflow image/video processing actors.
//!
//! This crate is Wasm-safe — no system dependencies, no threads, no filesystem.
//!
//! # Features
//!
//! - `simd` — Enables hand-tuned SIMD paths:
//!   - **aarch64**: NEON intrinsics for `row_rgba_to_gray` (8 pixels/iter)
//!   - **wasm32**: SIMD128 intrinsics for `row_rgba_to_gray` (4 pixels/iter)
//!     and `row_brightness` (4 pixels/iter)
//!
//! # Modules
//!
//! - [`format`] — Pixel format descriptors (RGBA8, RGB8, Gray8, etc.)
//! - [`color`] — Color space conversions (RGB ↔ HSV, grayscale, brightness/contrast/saturation)
//! - [`blend`] — Alpha blending and compositing (Normal, Multiply, Screen, Overlay, Add)
//! - [`resize`] — Bilinear interpolation (full-image and streaming row-by-row)
//! - [`chroma`] — Chroma key removal (green/blue screen)

pub mod blend;
pub mod chroma;
pub mod color;
pub mod format;
pub mod resize;

// SIMD acceleration modules (conditionally compiled)
#[cfg(all(feature = "simd", target_arch = "aarch64"))]
pub(crate) mod simd_color_neon;

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
pub(crate) mod simd_color_x86;

#[cfg(all(feature = "simd", target_arch = "wasm32"))]
pub(crate) mod simd_color_wasm;
