//! Reflow API Actors — auto-generated HTTP service integrations.
//!
//! Contains 88 API service modules with ~6,700 actor templates generated
//! by `api-schema-gen codegen`. Extracted from `reflow_components` to avoid
//! bloating build times for crates that don't need API integrations.

#[allow(clippy::all, unused_variables)]
pub mod api;

// Re-export types that the generated api code references as `crate::*`
pub use reflow_actor::{
    message::Message, Actor, ActorBehavior, ActorContext, ActorLoad, ActorPayload, ActorState,
    MemoryState, Port,
};

// Re-export registry functions
pub use api::api_registry::{get_api_actor_for_template, get_api_template_infos, ApiTemplateInfo};
