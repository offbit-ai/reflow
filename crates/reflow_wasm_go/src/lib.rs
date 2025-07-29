//! Reflow WebAssembly Go SDK
//! 
//! This crate provides the Go SDK for building Reflow actors as WebAssembly plugins
//! using TinyGo and Extism. The actual Go code is in the `sdk/` directory.
//! 
//! This Rust crate is mainly used for testing Go plugins and providing build utilities.

pub mod utils;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod minimal_test;

#[cfg(test)]
mod host_function_test;

#[cfg(test)]
mod basic_test;

#[cfg(test)]
mod go_integration_test;

#[cfg(test)]
mod debug_test;