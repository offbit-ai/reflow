#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
pub mod actor;
#[cfg(not(target_arch = "wasm32"))]
pub mod api_kit;
#[cfg(not(target_arch = "wasm32"))]
pub mod bridge;
pub mod connector;
#[cfg(not(target_arch = "wasm32"))]
pub mod discovery;
#[cfg(not(target_arch = "wasm32"))]
pub mod distributed_composition;
#[cfg(not(target_arch = "wasm32"))]
pub mod distributed_network;
pub mod graph;
mod helper;
pub mod input_events;
#[cfg(test)]
mod integration_tests;
pub mod message;
pub mod multi_graph;
pub mod network;
#[cfg(test)]
mod network_test;
pub mod ports;
#[cfg(not(target_arch = "wasm32"))]
pub mod proxy;
#[cfg(not(target_arch = "wasm32"))]
pub mod redis_state;
#[cfg(not(target_arch = "wasm32"))]
pub mod router;
#[cfg(not(target_arch = "wasm32"))]
pub mod script_discovery;
// SubgraphActor uses tokio::sync::watch + tokio::select! for its
// inbound routing loop. Native-only until those are replaced with
// flume signals + futures::select for the wasm path.
#[cfg(not(target_arch = "wasm32"))]
pub mod subgraph;
pub mod template;
pub mod tracing;
pub mod types;
pub mod websocket_rpc;

// Re-export stream infrastructure from reflow_actor for use by reflow_server
pub use reflow_actor::stream;

// Export WASM bindings
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

// Re-export actor system types for WASM
#[cfg(target_arch = "wasm32")]
pub use actor::{ActorLoad, BrowserActorContext, MemoryState};

// Re-export network types for WASM
#[cfg(target_arch = "wasm32")]
pub use network::{GraphNetwork, Network};

// Re-export multi_graph types for WASM (under multi_graph namespace)
#[cfg(target_arch = "wasm32")]
pub use multi_graph::wasm_bindings::*;
