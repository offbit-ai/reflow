//! A lightweight JavaScript runtime built on top of the Boa JavaScript engine.
//!
//! This crate provides a JavaScript runtime that can be used to execute JavaScript code
//! in a secure and controlled environment. It is designed to be lightweight, extensible,
//! and secure.
//!
//! # Features
//!
//! - Modern JavaScript support (ES2021+)
//! - Async/await and Promise support
//! - Console API
//! - Timer functions (setTimeout, setInterval)
//! - File system access (optional)
//! - Network access (optional)
//! - Module system (optional)
//! - Security model with fine-grained permissions

// Re-export the main types
pub use crate::runtime::{JsRuntime, RuntimeConfig};
pub use boa_engine::{JsValue, JsError, JsResult};

pub mod state;
// Modules
pub mod runtime;
pub mod async_runtime;
pub mod console;
pub mod timers;
pub mod security;

// These modules are not fully implemented yet
pub mod fs;
pub mod networking;
pub mod modules;
