//! Unified Reflow runtime facade.
//!
//! `reflow_rt` is the user-facing crate for building and running Reflow graphs.
//! It intentionally stays thin: the underlying crates remain independently
//! usable, while this crate provides one stable import surface and feature map
//! for applications.

pub use reflow_actor_macro::{actor, actor_display};
pub use reflow_actor;
pub use reflow_actor as actor_runtime;
pub use reflow_assets;
pub use reflow_assets as assets;
pub use reflow_components;
pub use reflow_components as components;
pub use reflow_graph;
pub use reflow_graph as graph;
pub use reflow_network;
pub use reflow_network as network;
pub use reflow_pixel;
pub use reflow_pixel as pixel;
pub use reflow_sdf;
pub use reflow_sdf as sdf;
pub use reflow_shader;
pub use reflow_shader as shader;
pub use reflow_vector;
pub use reflow_vector as vector;

#[cfg(feature = "api-services")]
pub use reflow_api_services;
#[cfg(feature = "api-services")]
pub use reflow_api_services as api_services;
#[cfg(feature = "ml")]
pub use reflow_asset_registry;
#[cfg(feature = "ml")]
pub use reflow_asset_registry as asset_registry;
#[cfg(feature = "ml")]
pub use reflow_cv_ops;
#[cfg(feature = "ml")]
pub use reflow_cv_ops as cv_ops;
#[cfg(feature = "av-core")]
pub use reflow_dsp;
#[cfg(feature = "av-core")]
pub use reflow_dsp as dsp;
#[cfg(feature = "ml")]
pub use reflow_litert;
#[cfg(feature = "ml")]
pub use reflow_litert as litert;
#[cfg(feature = "media")]
pub use reflow_media_codec;
#[cfg(feature = "media")]
pub use reflow_media_codec as media_codec;
#[cfg(feature = "media")]
pub use reflow_media_types;
#[cfg(feature = "media")]
pub use reflow_media_types as media_types;
#[cfg(feature = "ml")]
pub use reflow_ml_ops;
#[cfg(feature = "ml")]
pub use reflow_ml_ops as ml_ops;
#[cfg(feature = "ml")]
pub use reflow_taskpacks;
#[cfg(feature = "ml")]
pub use reflow_taskpacks as taskpacks;

/// Common imports for authoring Reflow actors, graphs, and networks.
///
/// This prelude is intentionally small. For specialized APIs, import through
/// the crate namespace re-exports such as `reflow_rt::components`,
/// `reflow_rt::media_types`, or `reflow_rt::taskpacks`.
pub mod prelude {
    pub use reflow_actor_macro::{actor, actor_display};
    pub use reflow_actor::{
        Actor, ActorBehavior, ActorChannel, ActorConfig, ActorContext, ActorLoad, ActorPayload,
        ActorState, MemoryState, Port,
        message::Message,
        stream::{StreamFrame, StreamHandle},
    };
    pub use reflow_components::{get_actor_for_template, get_template_mapping};
    pub use reflow_graph::{Graph, types::GraphExport};
    #[cfg(target_arch = "wasm32")]
    pub use reflow_network::network::GraphNetwork;
    pub use reflow_network::{
        network::{Network, NetworkConfig},
        subgraph::SubgraphActor,
        template::{DisplayComponent, NodeTemplate, TemplateCatalog, TemplateRegistry},
    };
}
