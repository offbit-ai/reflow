pub mod discovery;
pub mod extractor;
pub mod factory;
pub mod registry;
pub mod script_actor_trait;
pub mod types;
// pub mod actor_bridge;  // TODO: Re-enable when needed for simple script actors

// MicroSandbox integration - core component for secure script execution
pub mod microsandbox_runtime;

#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;

pub use discovery::*;
pub use extractor::*;
pub use factory::*;
pub use registry::*;
pub use script_actor_trait::ScriptActor;
pub use types::*;
// pub use actor_bridge::ScriptActorBridge;
