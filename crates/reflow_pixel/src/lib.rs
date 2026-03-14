//! Pure-Rust pixel operations for Reflow image/video processing actors.
//!
//! This crate is Wasm-safe — no system dependencies, no threads, no filesystem.
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
