//! System actors — ECS systems that process AssetDB components.
//!
//! Components (data) live in the AssetDB. Systems (logic) are DAG actors.
//!
//! Scene-specific systems are prefixed "Scene" (physics, camera, lights, materials).
//! Universal systems (tween, timeline, state machine, expression) work across domains.

// Scene systems (game/3D)
mod camera;
mod lights;
mod material;
mod physics;

// Universal systems (motion design, interactive animation, design engineering)
mod expression;
mod state_machine;
mod timeline;
mod tween;

pub use camera::SceneCameraSystemActor;
pub use lights::SceneLightCollectorActor;
pub use material::SceneMaterialSystemActor;
pub use physics::ScenePhysicsSystemActor;

pub use expression::ExpressionSystemActor;
pub use state_machine::StateMachineSystemActor;
pub use timeline::TimelineSystemActor;
pub use tween::TweenSystemActor;
