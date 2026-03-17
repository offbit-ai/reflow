//! Scene system actors — ECS systems that process AssetDB components.
//!
//! Components (data) live in the AssetDB. Systems (logic) are DAG actors
//! that query components, process them, and write results back.
//!
//! Prefixed with "Scene" to distinguish from non-game Reflow usage
//! (data pipelines, ETL, media processing, etc.).

mod camera;
mod lights;
mod material;
mod physics;

pub use camera::SceneCameraSystemActor;
pub use lights::SceneLightCollectorActor;
pub use material::SceneMaterialSystemActor;
pub use physics::ScenePhysicsSystemActor;
