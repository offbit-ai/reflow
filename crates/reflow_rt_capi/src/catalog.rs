//! Template-catalog + subgraph bindings.
//!
//! `rfl_template_actor_new` / `rfl_template_list_json` are always
//! compiled — they consult the pack registry first. The `components`
//! feature adds a fallback to the bundled `reflow_components` catalog
//! and enables subgraph embedding (which needs the catalog to resolve
//! internal node references).

#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

#[cfg(feature = "components")]
use std::ffi::c_void;
#[cfg(feature = "components")]
use std::sync::Arc;

#[cfg(feature = "components")]
use reflow_rt::actor_runtime::Actor;
#[cfg(feature = "components")]
use reflow_rt::graph::types::GraphExport;
#[cfg(feature = "components")]
use reflow_rt::network::subgraph::SubgraphActor;

use crate::actor::rfl_actor;
#[cfg(feature = "components")]
use crate::rfl_status;
use crate::set_last_error;

// ─── template catalog ──────────────────────────────────────────────────────

/// Instantiate an actor from a template id.
///
/// Lookup order:
/// 1. Templates registered by loaded `.rflpack` packs (see
///    `rfl_pack_load`).
/// 2. The bundled `reflow_components` catalog, if this build includes the
///    `components` feature (on by default).
///
/// Returns an `rfl_actor*` that can be handed to
/// `rfl_network_register_actor`. Returns NULL with
/// `rfl_last_error_message` set if no loaded pack and no bundled catalog
/// owns the id.
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

    if let Some(actor) = reflow_pack_loader::instantiate(tid) {
        return Box::into_raw(Box::new(rfl_actor { inner: actor }));
    }

    #[cfg(feature = "components")]
    {
        if let Some(actor) = reflow_components::get_actor_for_template(tid) {
            return Box::into_raw(Box::new(rfl_actor { inner: actor }));
        }
    }

    set_last_error(format!(
        "unknown template id '{tid}' — no loaded pack or bundled catalog entry"
    ));
    std::ptr::null_mut()
}

/// Return a JSON array of every template id reachable through this
/// runtime — loaded packs plus (when compiled) the bundled catalog.
/// Caller frees via `rfl_string_free`.
#[no_mangle]
pub extern "C" fn rfl_template_list_json() -> *mut c_char {
    crate::clear_last_error();
    let mut ids: Vec<String> = reflow_pack_loader::PACK_REGISTRY.template_ids();

    #[cfg(feature = "components")]
    {
        for k in reflow_components::get_template_mapping().keys() {
            if !ids.contains(k) {
                ids.push(k.clone());
            }
        }
    }
    ids.sort();

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

/// Build a SubgraphActor from a `GraphExport` JSON document. Each
/// component referenced inside the export is resolved against the pack
/// registry first, then the bundled `reflow_components` catalog.
/// Returns NULL on parse error or on unknown component references.
///
/// The resulting `rfl_actor*` can be handed to `rfl_network_register_actor`
/// exactly like any other actor template.
///
/// Requires the `components` feature (gives us `SubgraphActor`).
#[cfg(feature = "components")]
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

    // Resolve every referenced component. Packs win over the bundled
    // catalog when both claim the same id — same precedence as
    // `rfl_template_actor_new`.
    let mut actors: std::collections::HashMap<String, Arc<dyn Actor>> =
        std::collections::HashMap::new();
    for node in export.processes.values() {
        if actors.contains_key(&node.component) {
            continue;
        }
        let resolved = reflow_pack_loader::instantiate(&node.component)
            .or_else(|| reflow_components::get_actor_for_template(&node.component));
        match resolved {
            Some(a) => {
                actors.insert(node.component.clone(), a);
            }
            None => {
                set_last_error(format!(
                    "subgraph references unknown component '{}' — register it \
                     on the parent network first, load a pack that owns it, \
                     or expose a builder API",
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
#[cfg(feature = "components")]
#[allow(dead_code)]
fn _assert_types() {
    let _: Option<*mut c_void> = None;
    let _: rfl_status = rfl_status::Ok;
}
