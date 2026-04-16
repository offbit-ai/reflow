//! Zeal-Compatible Reflow Components
//!
//! Native actor implementations for Zeal workflow templates.
//! Script execution (JavaScript, Python, SQL, etc.) is handled by dynASB
//! via ComponentSpec::Script — this crate only contains native actors.
//!
//! The `api` feature (enabled by default) includes 88 generated API service
//! modules with 6,697 actor templates. Disable it for faster test builds:
//!   cargo test -p reflow_server --no-default-features
//!
//! The component catalog contains legacy/generated actors and GPU resource
//! holders where several style-oriented clippy lints are noisy under a
//! workspace-wide `-D warnings` sweep. Keep this baseline local to the catalog
//! while functional warnings and compile errors still fail normally.
#![allow(
    dead_code,
    unused_variables,
    clippy::assign_op_pattern,
    clippy::collapsible_match,
    clippy::doc_overindented_list_items,
    clippy::excessive_precision,
    clippy::explicit_counter_loop,
    clippy::field_reassign_with_default,
    clippy::get_first,
    clippy::len_zero,
    clippy::manual_checked_ops,
    clippy::manual_clamp,
    clippy::manual_is_multiple_of,
    clippy::manual_repeat_n,
    clippy::manual_unwrap_or,
    clippy::map_entry,
    clippy::needless_borrow,
    clippy::needless_borrows_for_generic_args,
    clippy::needless_range_loop,
    clippy::ptr_arg,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::unnecessary_cast,
    clippy::unnecessary_map_or,
    clippy::useless_format
)]

pub mod animation;
#[cfg(feature = "api_services")]
#[cfg(not(clippy))]
pub use reflow_api::api;
pub mod assets;
pub mod flow_control;
pub mod gpu;
#[cfg(feature = "window-events")]
pub mod input;
pub mod integration;
pub mod io;
pub mod logic;
pub mod math;
pub mod media;
#[cfg(feature = "ml")]
pub mod ml {
    pub use reflow_cv_ops::*;
    pub use reflow_ml_ops::*;
    pub use reflow_taskpacks::*;
}
pub mod procedural;
pub mod registry;
pub mod scene;
pub mod stream_ops;
pub mod systems;
pub mod text;
pub mod transform;
pub mod vector;

#[cfg(test)]
mod tests;

// Re-export common types
pub use reflow_actor::{
    message::Message, Actor, ActorBehavior, ActorContext, ActorLoad, ActorPayload, ActorState,
    MemoryState, Port,
};

// Re-export registry functions
pub use registry::{get_actor_for_template, get_template_mapping};

// Re-export API template metadata for ZIP registration (only with api feature)
#[cfg(feature = "api_services")]
#[cfg(not(clippy))]
pub use reflow_api::{get_api_actor_for_template, get_api_template_infos, ApiTemplateInfo};

// Stubs when api feature is disabled — lets dependents compile without the heavy API modules
#[cfg(not(feature = "api_services"))]
mod api_stubs {
    use std::sync::Arc;

    pub struct ApiTemplateInfo;

    pub fn get_api_template_infos() -> &'static [ApiTemplateInfo] {
        &[]
    }

    pub fn get_api_actor_for_template(_template_id: &str) -> Option<Arc<dyn crate::Actor>> {
        None
    }
}

#[cfg(not(feature = "api_services"))]
pub use api_stubs::{get_api_actor_for_template, get_api_template_infos, ApiTemplateInfo};
