//! GPU compute and rendering actors.
//!
//! - [`sdf`] — Signed Distance Function primitives, operations, transforms,
//!   materials, scene composition, and GPU ray march rendering.
//! - [`scene_render`] — Rasterization renderer for mesh-based scenes.
//! - [`font_atlas`] — Shared font atlas utilities (SDF glyph generation).
//! - [`font_load`] — FontLoadActor (loads TTF/OTF from AssetDB).
//! - [`glyph_atlas`] — GlyphAtlasActor (builds SDF glyph atlas).

#[cfg(feature = "gpu")]
pub mod context;
pub mod font_atlas;
pub mod font_load;
pub mod glyph_atlas;
#[cfg(feature = "gpu")]
pub mod scene_render;
pub mod sdf;
#[cfg(feature = "gpu")]
pub mod sdf_2d;
pub mod post_process;
pub mod shader;
