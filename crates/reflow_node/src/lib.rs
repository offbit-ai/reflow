//! Node.js bindings for reflow_network using Neon
//!
//! This crate provides Node.js bindings for the reflow_network library,
//! offering enhanced capabilities compared to the WASM version including:
//! - Full native async support via Tokio
//! - Access to filesystem operations
//! - Full networking capabilities
//! - Multi-threaded actor execution

use neon::prelude::*;

mod runtime;
mod neon_bindings;

// Import all the binding functions
use neon_bindings::{
    network::{create_network, create_graph_network},
    graph::{create_graph, create_graph_history},
    actor::create_actor,
    multi_graph::{
        create_workspace, create_graph_composer, create_graph_loader, create_graph_metadata, create_graph_normalizer, create_graph_validator,
        create_namespace_manager
    },
    errors::{
        create_composition_error, create_validation_error,
        create_load_error, create_namespace_error
    },
};

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    // Initialize the async runtime
    runtime::init_runtime();
    
    // Export basic functionality
    cx.export_function("init_panic_hook", init_panic_hook)?;
    
    // Export Network bindings - enhanced versions with full Node.js capabilities
    cx.export_function("Network", create_network)?;
    
    cx.export_function("GraphNetwork", create_graph_network)?;
    
    // Export Graph bindings - clean names without WASM* prefix
    cx.export_function("Graph", create_graph)?;
    cx.export_function("GraphHistory", create_graph_history)?;
    
    // Export Actor bindings
    cx.export_function("Actor", create_actor)?;
    
    // Export Multi-graph bindings - workspace and composition features
    cx.export_function("Workspace", create_workspace)?;
    // cx.export_function("MultiGraphNetwork", create_multi_graph_network)?;
    cx.export_function("NamespaceManager", create_namespace_manager)?;
    cx.export_function("GraphComposer", create_graph_composer)?;
    cx.export_function("GraphLoader", create_graph_loader)?;
    cx.export_function("GraphMetadata", create_graph_metadata)?;
    cx.export_function("GraphNormalizer", create_graph_normalizer)?;
    cx.export_function("GraphValidator", create_graph_normalizer)?;
    // cx.export_function("GraphDependencyManager", create_graph_dependency_manager)?;
    
    // Export Error types - clean names, no WASM* prefix
    cx.export_function("CompositionError", create_composition_error)?;
    cx.export_function("ValidationError", create_validation_error)?;
    cx.export_function("LoadError", create_load_error)?;
    cx.export_function("NamespaceError", create_namespace_error)?;
    
    Ok(())
}

/// Initialize panic hook for better error reporting
fn init_panic_hook(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("Reflow Node panic: {}", info);
    }));
    Ok(cx.undefined())
}
