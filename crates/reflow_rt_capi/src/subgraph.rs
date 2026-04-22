//! Subgraph builder — embed a `GraphExport` as a first-class actor,
//! plugging in custom (callback or template-sourced) actors for the
//! components referenced inside.
//!
//! Contrast with `rfl_subgraph_actor_new_from_json` in `catalog.rs`, which
//! resolves every referenced component from the bundled
//! `reflow_components` catalog. Use this builder when the subgraph
//! references components that only exist in your SDK (e.g. callback
//! actors registered on the parent network).

#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::Arc;

use reflow_rt::actor_runtime::Actor;
use reflow_rt::graph::types::GraphExport;
use reflow_rt::network::subgraph::SubgraphActor;

use crate::actor::rfl_actor;
use crate::{rfl_status, set_last_error};

/// Builder for a `SubgraphActor` with an explicit actor map.
pub struct rfl_subgraph_builder {
    export: GraphExport,
    actors: HashMap<String, Arc<dyn Actor>>,
}

/// Start a subgraph builder over a `GraphExport` JSON document. Returns
/// NULL on parse error.
#[no_mangle]
pub unsafe extern "C" fn rfl_subgraph_builder_new(
    graph_export_json: *const c_char,
) -> *mut rfl_subgraph_builder {
    crate::clear_last_error();
    if graph_export_json.is_null() {
        set_last_error("graph_export_json is null");
        return std::ptr::null_mut();
    }
    let s = match unsafe { CStr::from_ptr(graph_export_json) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("graph_export_json is not valid UTF-8");
            return std::ptr::null_mut();
        }
    };
    let export: GraphExport = match serde_json::from_str(s) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(format!("graph_export_json parse: {e}"));
            return std::ptr::null_mut();
        }
    };
    Box::into_raw(Box::new(rfl_subgraph_builder {
        export,
        actors: HashMap::new(),
    }))
}

/// Register an actor under `component_name`. The builder takes ownership
/// of the actor handle — do not free it afterwards. Replacing an existing
/// registration silently overwrites it.
#[no_mangle]
pub unsafe extern "C" fn rfl_subgraph_builder_register_actor(
    b: *mut rfl_subgraph_builder,
    component_name: *const c_char,
    actor: *mut rfl_actor,
) -> rfl_status {
    crate::clear_last_error();
    if b.is_null() || actor.is_null() {
        return rfl_status::NullArg;
    }
    let name = match unsafe { CStr::from_ptr(component_name) }.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => {
            set_last_error("component_name is not valid UTF-8");
            return rfl_status::InvalidUtf8;
        }
    };
    let actor_inner = unsafe { Box::from_raw(actor) }.inner;
    unsafe { &mut *b }.actors.insert(name, actor_inner);
    rfl_status::Ok
}

/// Resolve any still-unregistered components from the bundled catalog.
/// Gated on the `components` feature; no-op (returns Ok) otherwise.
#[no_mangle]
pub unsafe extern "C" fn rfl_subgraph_builder_fill_from_catalog(
    b: *mut rfl_subgraph_builder,
) -> rfl_status {
    crate::clear_last_error();
    if b.is_null() {
        return rfl_status::NullArg;
    }

    #[cfg(feature = "components")]
    {
        let builder = unsafe { &mut *b };
        // Collect component names referenced in the export but not yet
        // registered on the builder.
        let needed: Vec<String> = builder
            .export
            .processes
            .values()
            .map(|n| n.component.clone())
            .filter(|c| !builder.actors.contains_key(c))
            .collect();
        for name in needed {
            match reflow_components::get_actor_for_template(&name) {
                Some(actor) => {
                    builder.actors.insert(name, actor);
                }
                None => {
                    set_last_error(format!(
                        "subgraph references unknown component '{name}' — register \
                         it on the builder with rfl_subgraph_builder_register_actor"
                    ));
                    return rfl_status::Runtime;
                }
            }
        }
    }

    #[cfg(not(feature = "components"))]
    let _ = b;

    rfl_status::Ok
}

/// Build the subgraph into an actor handle. Consumes the builder.
#[no_mangle]
pub unsafe extern "C" fn rfl_subgraph_builder_build(
    b: *mut rfl_subgraph_builder,
) -> *mut rfl_actor {
    crate::clear_last_error();
    if b.is_null() {
        set_last_error("builder is null");
        return std::ptr::null_mut();
    }
    let builder = unsafe { Box::from_raw(b) };

    // Check every component referenced by the export has an actor.
    for node in builder.export.processes.values() {
        if !builder.actors.contains_key(&node.component) {
            set_last_error(format!(
                "subgraph references unregistered component '{}'",
                node.component
            ));
            return std::ptr::null_mut();
        }
    }

    match SubgraphActor::from_graph_export(&builder.export, builder.actors) {
        Ok(sg) => Box::into_raw(Box::new(rfl_actor {
            inner: Arc::new(sg) as Arc<dyn Actor>,
        })),
        Err(e) => {
            set_last_error(format!("subgraph build: {e}"));
            std::ptr::null_mut()
        }
    }
}

/// Abandon a builder without building. Safe on NULL.
#[no_mangle]
pub unsafe extern "C" fn rfl_subgraph_builder_free(b: *mut rfl_subgraph_builder) {
    if !b.is_null() {
        drop(unsafe { Box::from_raw(b) });
    }
}
