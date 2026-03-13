//! Zeal-Compatible Reflow Components
//!
//! Native actor implementations for Zeal workflow templates.
//! Script execution (JavaScript, Python, SQL, etc.) is handled by dynASB
//! via ComponentSpec::Script — this crate only contains native actors.
//!
//! The `api` feature (enabled by default) includes 88 generated API service
//! modules with 6,697 actor templates. Disable it for faster test builds:
//!   cargo test -p reflow_server --no-default-features

#[cfg(feature = "api")]
#[allow(clippy::all, unused_variables)]
pub mod api;
pub mod flow_control;
pub mod integration;
pub mod logic;
pub mod media;
pub mod registry;
pub mod transform;

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
pub use api::api_registry::{get_api_actor_for_template, get_api_template_infos, ApiTemplateInfo};

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
