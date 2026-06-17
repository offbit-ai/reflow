//! # Reflow Tracing Server (library surface)
//!
//! Exposes the tracing server's building blocks so it can be embedded — most
//! importantly in integration tests and SDK collectors — in addition to being
//! run as the `reflow_tracing` binary.

pub mod config;
pub mod protocol;
pub mod server;
pub mod storage;
