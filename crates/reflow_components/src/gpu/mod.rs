//! GPU compute and rendering actors.
//!
//! - [`sdf`] — Signed Distance Function primitives, operations, transforms,
//!   materials, scene composition, and GPU ray march rendering.
//! - [`scene_render`] — Rasterization renderer for mesh-based scenes.

#[cfg(feature = "gpu")]
pub mod context;
#[cfg(feature = "gpu")]
pub mod scene_render;
pub mod sdf;
#[cfg(feature = "gpu")]
pub mod sdf_2d;
