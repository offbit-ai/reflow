#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
pub mod discovery;
#[cfg(not(target_arch = "wasm32"))]
pub mod distributed_network;
#[cfg(not(target_arch = "wasm32"))]
pub mod bridge;
#[cfg(not(target_arch = "wasm32"))]
pub mod router;
#[cfg(not(target_arch = "wasm32"))]
pub mod proxy;
pub mod network;
pub mod connector;
pub mod types;
pub mod ports;
pub mod message;
pub mod actor;
mod helper;
pub mod graph;
pub mod multi_graph;
pub mod tracing;
#[cfg(not(target_arch = "wasm32"))]
pub mod api_kit;
#[cfg(not(target_arch = "wasm32"))]
pub mod script_discovery;
#[cfg(not(target_arch = "wasm32"))]
pub mod websocket_rpc;
#[cfg(not(target_arch = "wasm32"))]
pub mod redis_state;
#[cfg(test)]
mod network_test;

// Export WASM bindings
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

// Re-export actor system types for WASM
#[cfg(target_arch = "wasm32")]
pub use actor::{
    ActorLoad, 
    MemoryState, 
    BrowserActorContext, 
};

// Re-export network types for WASM
#[cfg(target_arch = "wasm32")]
pub use network::{
    Network,
    GraphNetwork
};

// Re-export multi_graph types for WASM (under multi_graph namespace)
#[cfg(target_arch = "wasm32")]
pub use multi_graph::wasm_bindings::*;
