//! Template-catalog + subgraph bindings.
//!
//! Gated on the `components` Cargo feature. When enabled, the bundled
//! `reflow_components` catalog is exposed to C callers so SDKs don't have
//! to re-author every built-in actor as a callback. Embedded subgraphs
//! also resolve their component references against this same catalog.
//!
//! Build without `components` if you want the runtime surface only.

#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]
#![cfg(feature = "components")]

use std::ffi::{c_void, CStr, CString};
use std::os::raw::c_char;
use std::sync::Arc;

use reflow_rt::actor_runtime::Actor;
use reflow_rt::graph::types::GraphExport;
use reflow_rt::network::subgraph::SubgraphActor;

use crate::actor::rfl_actor;
use crate::{rfl_status, set_last_error};

// ─── template catalog ──────────────────────────────────────────────────────

/// Instantiate an actor from the bundled `reflow_components` catalog.
///
/// Returns an `rfl_actor*` that can be handed to `rfl_network_register_actor`
/// exactly like a callback-driven actor. Returns NULL if the template id is
/// not recognised.
///
/// Available only when the crate is compiled with the `components` feature
/// (on by default).
#[no_mangle]
pub unsafe extern "C" fn rfl_template_actor_new(template_id: *const c_char) -> *mut rfl_actor {
    crate::clear_last_error();
    if template_id.is_null() {
        set_last_error("template_id is null");
        return std::ptr::null_mut();
    }
    let tid = match unsafe { CStr::from_ptr(template_id) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("template_id is not valid UTF-8");
            return std::ptr::null_mut();
        }
    };
    match reflow_components::get_actor_for_template(tid) {
        Some(actor) => Box::into_raw(Box::new(rfl_actor { inner: actor })),
        None => {
            set_last_error(format!("unknown template id '{tid}'"));
            std::ptr::null_mut()
        }
    }
}

/// Return a JSON array of every template id the bundled catalog knows
/// about. Caller frees via `rfl_string_free`.
#[no_mangle]
pub extern "C" fn rfl_template_list_json() -> *mut c_char {
    crate::clear_last_error();
    let ids: Vec<String> = reflow_components::get_template_mapping()
        .keys()
        .cloned()
        .collect();
    match serde_json::to_string(&ids) {
        Ok(s) => CString::new(s)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(e) => {
            set_last_error(format!("template list serialize: {e}"));
            std::ptr::null_mut()
        }
    }
}

// ─── subgraph embedding ────────────────────────────────────────────────────

/// Build a SubgraphActor from a `GraphExport` JSON document. Each component
/// referenced inside the export is resolved against the bundled
/// `reflow_components` catalog. Returns NULL on parse error or on unknown
/// component references.
///
/// The resulting `rfl_actor*` can be handed to `rfl_network_register_actor`
/// exactly like any other actor template.
#[no_mangle]
pub unsafe extern "C" fn rfl_subgraph_actor_new_from_json(
    graph_export_json: *const c_char,
) -> *mut rfl_actor {
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

    // Collect every referenced component name and resolve from catalog.
    let mut actors: std::collections::HashMap<String, Arc<dyn Actor>> =
        std::collections::HashMap::new();
    for (_id, node) in &export.processes {
        if actors.contains_key(&node.component) {
            continue;
        }
        match reflow_components::get_actor_for_template(&node.component) {
            Some(a) => {
                actors.insert(node.component.clone(), a);
            }
            None => {
                set_last_error(format!(
                    "subgraph references unknown component '{}' — register it \
                     on the parent network first or expose a builder API",
                    node.component
                ));
                return std::ptr::null_mut();
            }
        }
    }

    match SubgraphActor::from_graph_export(&export, actors) {
        Ok(sg) => Box::into_raw(Box::new(rfl_actor {
            inner: Arc::new(sg) as Arc<dyn Actor>,
        })),
        Err(e) => {
            set_last_error(format!("subgraph build: {e}"));
            std::ptr::null_mut()
        }
    }
}

// Suppress unused-import lint when `components` is enabled but nothing
// touches these re-exports in this file.
#[allow(dead_code)]
fn _assert_types() {
    let _: Option<*mut c_void> = None;
    let _: rfl_status = rfl_status::Ok;
}
