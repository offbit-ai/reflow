#[cfg(target_arch = "wasm32")]
pub mod browser_client;
#[cfg(not(target_arch = "wasm32"))]
pub mod client;
#[cfg(not(target_arch = "wasm32"))]
pub mod dynasb_engine;
#[cfg(not(target_arch = "wasm32"))]
pub mod script_actor;
pub mod types;

#[cfg(test)]
mod tests;

#[cfg(target_arch = "wasm32")]
pub use browser_client::*;
#[cfg(not(target_arch = "wasm32"))]
pub use client::*;
#[cfg(not(target_arch = "wasm32"))]
pub use dynasb_engine::*;
#[cfg(not(target_arch = "wasm32"))]
pub use script_actor::*;
pub use types::*;
