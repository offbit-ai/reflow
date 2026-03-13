//! Zeal-Compatible Reflow Components
//!
//! Native actor implementations for Zeal workflow templates.
//! Script execution (JavaScript, Python, SQL, etc.) is handled by dynASB
//! via ComponentSpec::Script — this crate only contains native actors.

#[allow(clippy::all)]
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

// Re-export API template metadata for ZIP registration
pub use api::api_registry::{get_api_actor_for_template, get_api_template_infos, ApiTemplateInfo};
