//! Zeal-Compatible Reflow Components
//!
//! Native actor implementations for Zeal workflow templates.
//! Script execution (JavaScript, Python, SQL, etc.) is handled by dynASB
//! via ComponentSpec::Script — this crate only contains native actors.
//!
//! The `api` feature (enabled by default) includes 88 generated API service
//! modules with 6,697 actor templates. Disable it for faster test builds:
//!   cargo test -p reflow_server --no-default-features

pub mod animation;
#[cfg(feature = "api")]
pub use reflow_api::api;
pub mod assets;
pub mod flow_control;
pub mod gpu;
#[cfg(feature = "window-events")]
pub mod input;
pub mod integration;
pub mod io;
pub mod logic;
pub mod math;
pub mod media;
pub mod procedural;
pub mod registry;
pub mod scene;
pub mod stream_ops;
pub mod systems;
pub mod text;
pub mod transform;
pub mod vector;

#[cfg(test)]
mod tests;

// Re-export common types
pub use reflow_actor::{
    message::Message, Actor, ActorBehavior, ActorContext, ActorLoad, ActorPayload, ActorState,
    MemoryState, Port,
};

// Re-export registry functions
pub use registry::{get_actor_for_template, get_template_mapping};

// Re-export API template metadata for ZIP registration (only with api feature)
#[cfg(feature = "api")]
pub use reflow_api::{get_api_actor_for_template, get_api_template_infos, ApiTemplateInfo};

// Stubs when api feature is disabled — lets dependents compile without the heavy API modules
#[cfg(not(feature = "api"))]
mod api_stubs {
    use std::sync::Arc;

    pub struct ApiTemplateInfo;

    pub fn get_api_template_infos() -> &'static [ApiTemplateInfo] {
        &[]
    }

    pub fn get_api_actor_for_template(_template_id: &str) -> Option<Arc<dyn crate::Actor>> {
        None
    }
}

#[cfg(not(feature = "api"))]
pub use api_stubs::{get_api_actor_for_template, get_api_template_infos, ApiTemplateInfo};
