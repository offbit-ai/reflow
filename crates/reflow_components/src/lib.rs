//! Zeal-Compatible Reflow Components
//!
//! Native actor implementations for Zeal workflow templates.
//! Script execution (JavaScript, Python, SQL, etc.) is handled by dynASB
//! via ComponentSpec::Script — this crate only contains native actors.

pub mod flow_control;
pub mod integration;
pub mod logic;
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
