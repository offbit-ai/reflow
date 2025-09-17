//! Zeal-Compatible Reflow Components
//!
//! This crate provides actor implementations that are fully compatible with Zeal workflow templates.
//! Each actor is designed to work seamlessly with the metadata and configuration provided by Zeal.

pub mod integration;
pub mod flow_control;
pub mod data_operations;
pub mod data_ops;
pub mod rules;
pub mod scripting;
pub mod registry;

#[cfg(test)]
pub mod integration_tests;

#[cfg(test)]
pub mod end_to_end_tests;

// Re-export common types
pub use reflow_actor::{
    Actor, ActorContext, ActorLoad, ActorBehavior, ActorPayload, ActorState, MemoryState, Port,
    message::Message,
};

// Re-export registry functions
pub use registry::{get_actor_for_template, get_template_mapping};