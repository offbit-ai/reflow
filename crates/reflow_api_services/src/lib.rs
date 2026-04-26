//! Reflow API Actors — auto-generated HTTP service integrations.
//!
//! Contains 88 API service modules with ~6,700 actor templates generated
//! by `api-schema-gen codegen`. Extracted from `reflow_components` to avoid
//! bloating build times for crates that don't need API integrations.

// #[allow(clippy::all, unused_variables)]
#[cfg(not(clippy))]
pub mod api;

// Re-export types that the generated api code references as `crate::*`
pub use reflow_actor::{
    message::Message, Actor, ActorBehavior, ActorContext, ActorLoad, ActorPayload, ActorState,
    MemoryState, Port,
};

/// Extension trait so the generated API actors compile cross-target.
///
/// `reqwest::ClientBuilder::timeout` is native-only — it doesn't exist
/// on the wasm32 fetch backend (the Fetch API ignores per-request
/// timeouts; ditto on the ClientBuilder). Rather than cfg-gate every
/// one of the ~6,700 generated callsites, we replace `.timeout(d)`
/// with `.timeout_compat(d)` and route through this trait, which is
/// a passthrough on native and a no-op on wasm.
pub trait ClientBuilderExt: Sized {
    fn timeout_compat(self, duration: std::time::Duration) -> Self;
}

#[cfg(not(target_arch = "wasm32"))]
impl ClientBuilderExt for reqwest::ClientBuilder {
    #[inline]
    fn timeout_compat(self, duration: std::time::Duration) -> Self {
        self.timeout(duration)
    }
}

#[cfg(target_arch = "wasm32")]
impl ClientBuilderExt for reqwest::ClientBuilder {
    #[inline]
    fn timeout_compat(self, _duration: std::time::Duration) -> Self {
        self
    }
}

// Re-export registry functions
#[cfg(not(clippy))]
pub use api::api_registry::{get_api_actor_for_template, get_api_template_infos, ApiTemplateInfo};
