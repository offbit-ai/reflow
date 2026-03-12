//! Zeal-Compatible Reflow Components
//!
//! This crate provides actor implementations that are fully compatible with Zeal workflow templates.
//! Each actor is designed to work seamlessly with the metadata and configuration provided by Zeal.

pub mod data_operations;
pub mod data_ops;
pub mod flow_control;
pub mod integration;
pub mod registry;
pub mod rules;
pub mod scripting;

#[cfg(test)]
pub mod integration_tests;

#[cfg(test)]
pub mod end_to_end_tests;

// Re-export common types
pub use reflow_actor::{
    message::Message, Actor, ActorBehavior, ActorContext, ActorLoad, ActorPayload, ActorState,
    MemoryState, Port,
};

// Re-export registry functions
pub use registry::{get_actor_for_template, get_template_mapping};
