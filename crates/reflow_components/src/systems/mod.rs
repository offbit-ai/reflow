//! System actors — ECS systems that process AssetDB components.
//!
//! Components (data) live in the AssetDB. Systems (logic) are DAG actors.
//!
//! Scene-specific systems are prefixed "Scene" (physics, camera, lights, materials,
//! billboard, skybox, weather).
//! Universal systems (tween, timeline, state machine, behavior, layout, text) work across domains.

// Shared entity selector
mod selector;

// Scene systems (game/3D)
mod billboard;
mod camera;
mod lights;
mod material;
mod physics;
mod skybox;
mod weather;

// Universal systems (motion design, interactive animation, design engineering)
mod behavior;
mod layout_sync;
mod state_machine;
mod text_render;
mod text_sdf;
mod timeline;
mod tween;

pub use billboard::SceneBillboardSystemActor;
pub use camera::SceneCameraSystemActor;
pub use lights::SceneLightCollectorActor;
pub use material::SceneMaterialSystemActor;
pub use physics::ScenePhysicsSystemActor;
pub use skybox::SceneSkyboxSystemActor;
pub use weather::SceneWeatherSystemActor;

pub use behavior::BehaviorSystemActor;
pub use layout_sync::LayoutSyncSystemActor;
pub use state_machine::StateMachineSystemActor;
pub use text_render::TextRenderSystemActor;
pub use text_sdf::TextSdfSystemActor;
pub use timeline::TimelineSystemActor;
pub use tween::TweenSystemActor;
