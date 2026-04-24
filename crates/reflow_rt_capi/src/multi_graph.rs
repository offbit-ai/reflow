//! Multi-graph composition bindings.
//!
//! Compose N `GraphExport` JSON documents into a single runnable graph,
//! merging namespaces, resolving dependencies, and wiring cross-graph
//! connections. The result is itself a `GraphExport` — load it into a
//! `Network` via `rfl_graph_load_json` + `rfl_network_from_graph`.
//!
//! Request shape (JSON):
//!
//! ```json
//! {
//!   "graphs": [<GraphExport>, ...],
//!   "connections": [
//!     { "from": { "process": "ns/proc", "port": "out" },
//!       "to":   { "process": "ns/proc", "port": "in" } }
//!   ],
//!   "shared_resources": [
//!     { "name": "shared_db", "component": "tpl_db" }
//!   ],
//!   "properties": { "name": "composed" },
//!   "case_sensitive": false
//! }
//! ```

#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use reflow_rt::graph::types::GraphExport;
use reflow_rt::network::multi_graph::{
    CompositionConnection, CompositionEndpoint, GraphComposer, GraphComposition, GraphSource,
    SharedResource,
};
use serde::Deserialize;

use crate::runtime;
use crate::{clear_last_error, set_last_error};

#[derive(Debug, Deserialize)]
struct Request {
    #[serde(default)]
    graphs: Vec<GraphExport>,
    #[serde(default)]
    connections: Vec<ReqConnection>,
    #[serde(default, rename = "shared_resources")]
    shared_resources: Vec<ReqSharedResource>,
    #[serde(default)]
    properties: HashMap<String, serde_json::Value>,
    #[serde(default, rename = "case_sensitive")]
    case_sensitive: Option<bool>,
    #[serde(default)]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct ReqConnection {
    from: ReqEndpoint,
    to: ReqEndpoint,
    #[serde(default)]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct ReqEndpoint {
    process: String,
    port: String,
    #[serde(default)]
    index: Option<usize>,
}

impl From<ReqEndpoint> for CompositionEndpoint {
    fn from(r: ReqEndpoint) -> Self {
        CompositionEndpoint {
            process: r.process,
            port: r.port,
            index: r.index,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ReqSharedResource {
    name: String,
    component: String,
    #[serde(default)]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

impl From<ReqSharedResource> for SharedResource {
    fn from(r: ReqSharedResource) -> Self {
        SharedResource {
            name: r.name,
            component: r.component,
            metadata: r.metadata,
        }
    }
}

/// Compose N graph exports into a single `GraphExport` JSON document.
///
/// The returned string is a heap-allocated C string owned by the caller;
/// free with `rfl_string_free`. Returns NULL on failure; read the error
/// message via `rfl_last_error_message`.
#[no_mangle]
pub unsafe extern "C" fn rfl_compose_graphs(composition_json: *const c_char) -> *mut c_char {
    clear_last_error();
    if composition_json.is_null() {
        set_last_error("composition_json is null");
        return std::ptr::null_mut();
    }
    let s = match unsafe { CStr::from_ptr(composition_json) }.to_str() {
        Ok(v) => v,
        Err(_) => {
            set_last_error("composition_json is not valid UTF-8");
            return std::ptr::null_mut();
        }
    };
    let req: Request = match serde_json::from_str(s) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(format!("composition_json parse: {e}"));
            return std::ptr::null_mut();
        }
    };

    let composition = GraphComposition {
        sources: req
            .graphs
            .into_iter()
            .map(GraphSource::GraphExport)
            .collect(),
        connections: req
            .connections
            .into_iter()
            .map(|c| CompositionConnection {
                from: c.from.into(),
                to: c.to.into(),
                metadata: c.metadata,
            })
            .collect(),
        shared_resources: req.shared_resources.into_iter().map(Into::into).collect(),
        properties: req.properties,
        case_sensitive: req.case_sensitive,
        metadata: req.metadata,
    };

    let rt = runtime();
    let compose_res = rt.block_on(async {
        let mut composer = GraphComposer::new();
        composer.compose_graphs(composition).await
    });

    let composed = match compose_res {
        Ok(g) => g,
        Err(e) => {
            set_last_error(format!("compose_graphs: {e}"));
            return std::ptr::null_mut();
        }
    };

    let export = composed.export();
    match serde_json::to_string(&export) {
        Ok(s) => CString::new(s).map(|c| c.into_raw()).unwrap_or_else(|_| {
            set_last_error("composed graph JSON contained a NUL byte");
            std::ptr::null_mut()
        }),
        Err(e) => {
            set_last_error(format!("serialize composed graph: {e}"));
            std::ptr::null_mut()
        }
    }
}
