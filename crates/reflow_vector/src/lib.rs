//! 2D vector graphics for Reflow.
//!
//! Pure Rust, Wasm-safe. Provides:
//! - `Path2D` — bezier path representation with SVG `d` attribute parsing
//! - Shape primitives — rect, ellipse, polygon, star, line
//! - Path operations — transform, reverse, flatten, sample, bounding box
//! - Fill/stroke styles — solid, gradient, pattern

pub mod path;
pub mod shapes;
pub mod style;
pub mod transform;

pub use path::Path2D;
pub use shapes::*;
pub use style::*;
