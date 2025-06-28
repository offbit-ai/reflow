//! Multi-graph workspace example library
//! 
//! This module provides custom actors for demonstrating the multi-graph 
//! workspace discovery and composition system.

pub mod actors;

// Re-export the actors for easy access
pub use actors::{SimpleTimerActor, SimpleLoggerActor, DataGeneratorActor};
