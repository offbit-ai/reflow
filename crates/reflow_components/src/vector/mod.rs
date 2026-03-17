//! 2D vector graphics actors — shapes, paths, rasterization, compositing.

mod background;
mod blend_mode;
mod blur;
mod canvas;
mod rasterize;
mod shape;

pub use background::BackgroundActor;
pub use blend_mode::BlendModeActor;
pub use blur::GaussianBlurActor;
pub use canvas::Canvas2DActor;
pub use rasterize::VectorRasterizeActor;
pub use shape::Shape2DActor;
