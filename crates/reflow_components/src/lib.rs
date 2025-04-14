//! # Actor-Based Workflow Engine Standard Component Library
//!
//! This crate provides a comprehensive set of reusable components for the
//! actor model-based workflow engine. The components are organized into
//! categories based on their functionality.

pub mod flow_control;
pub mod data_operations;
pub mod synchronization;
pub mod error_handling;
pub mod integration;
pub mod utility;
pub mod state_management;

/// Re-export common types and traits used by components
pub use reflow_network::{
    actor::{Actor, ActorBehavior, ActorPayload, ActorState, MemoryState, Port},
    message::Message,
    network::Network,
};