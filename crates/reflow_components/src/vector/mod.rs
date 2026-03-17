//! 2D vector graphics actors — shapes, paths, rasterization, compositing.

mod blend_mode;
mod blur;
mod rasterize;
mod shape;

pub use blend_mode::BlendModeActor;
pub use blur::GaussianBlurActor;
pub use rasterize::VectorRasterizeActor;
pub use shape::Shape2DActor;
