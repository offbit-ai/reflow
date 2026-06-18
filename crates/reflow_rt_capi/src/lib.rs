//! C ABI bindings for the Reflow runtime.
//!
//! Design conventions
//! ==================
//!
//! Handles
//! -------
//! All objects exposed to C are **opaque pointers** created by `*_new` /
//! `*_load` functions and destroyed by the matching `*_free` function.
//! Passing NULL to a `*_free` function is a no-op (idempotent).
//!
//! Return values
//! -------------
//! Functions that can fail return an `rfl_status` enum. Output values are
//! written through out-parameters. On error, out-parameters are left
//! untouched and the last error message is available via
//! `rfl_last_error_message` (thread-local).
//!
//! Strings
//! -------
//! - C → Rust: null-terminated UTF-8 `const char *`.
//! - Rust → C: `char *` allocated by this library; caller frees with
//!   `rfl_string_free`. Never pass a Rust-allocated string to `free(3)`.
//!
//! Threading
//! ---------
//! Handles are `Send + Sync` and may be used from any thread once created.
//! The runtime owns a single multi-thread tokio executor that is spun up
//! on the first call that needs async work. `rfl_runtime_shutdown` tears
//! it down (called implicitly by the library destructor).
#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

mod actor;
pub use actor::*;

mod message;
pub use message::*;

mod stream;
pub use stream::*;

mod subgraph;
pub use subgraph::*;

mod multi_graph;
pub use multi_graph::*;

mod pack;
pub use pack::*;

mod catalog;
pub use catalog::*;

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::Arc;

use once_cell::sync::Lazy;
use parking_lot::Mutex as PlMutex;
use reflow_rt::actor_runtime::message::Message;
use reflow_rt::graph::{types::GraphExport, Graph};
use reflow_rt::network::connector::{ConnectionPoint, Connector, InitialPacket};
use reflow_rt::network::network::{Network, NetworkConfig, NetworkEvent};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

// ─── error handling ────────────────────────────────────────────────────────

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

pub(crate) fn set_last_error(msg: impl Into<String>) {
    let s = CString::new(msg.into()).unwrap_or_else(|_| CString::new("<invalid>").unwrap());
    LAST_ERROR.with(|cell| *cell.borrow_mut() = Some(s));
}

pub(crate) fn clear_last_error() {
    LAST_ERROR.with(|cell| *cell.borrow_mut() = None);
}

/// Returns the last thread-local error message as a newly allocated C string.
/// Caller frees with `rfl_string_free`. Returns NULL if there is no error.
#[no_mangle]
pub extern "C" fn rfl_last_error_message() -> *mut c_char {
    LAST_ERROR.with(|cell| match cell.borrow().as_ref() {
        Some(s) => s.clone().into_raw(),
        None => std::ptr::null_mut(),
    })
}

/// Free a string returned by this library. Passing NULL is a no-op.
#[no_mangle]
pub unsafe extern "C" fn rfl_string_free(s: *mut c_char) {
    if !s.is_null() {
        let _ = unsafe { CString::from_raw(s) };
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum rfl_status {
    /// Success.
    Ok = 0,
    /// A required argument was NULL.
    NullArg = -1,
    /// A C string was not valid UTF-8.
    InvalidUtf8 = -2,
    /// A JSON payload failed to parse or deserialize.
    InvalidJson = -3,
    /// The runtime refused the operation (see `rfl_last_error_message`).
    Runtime = -4,
    /// The network has already been started or is in a state that forbids
    /// the operation.
    InvalidState = -5,
}

fn to_status_runtime(e: impl std::fmt::Display) -> rfl_status {
    set_last_error(format!("{e}"));
    rfl_status::Runtime
}

// ─── runtime ───────────────────────────────────────────────────────────────

static RUNTIME: Lazy<PlMutex<Option<Arc<tokio::runtime::Runtime>>>> =
    Lazy::new(|| PlMutex::new(None));

pub(crate) fn runtime() -> Arc<tokio::runtime::Runtime> {
    let mut guard = RUNTIME.lock();
    if let Some(rt) = guard.as_ref() {
        return Arc::clone(rt);
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("reflow-capi")
        .build()
        .expect("failed to build tokio runtime for reflow_rt_capi");
    let rt = Arc::new(rt);
    *guard = Some(Arc::clone(&rt));
    rt
}

/// Explicitly tear down the shared tokio runtime.
///
/// Safe to call more than once. Normally this happens automatically on
/// library unload; call it from long-lived embedders that want to release
/// threads early.
#[no_mangle]
pub extern "C" fn rfl_runtime_shutdown() {
    if let Some(rt) = RUNTIME.lock().take() {
        // drop Arc; if we were the last, tokio threads wind down.
        drop(rt);
    }
}

// ─── graph ─────────────────────────────────────────────────────────────────

/// Opaque handle to a `reflow_graph::Graph`.
pub struct rfl_graph {
    inner: Graph,
}

/// Create a new empty graph.
///
/// `name` and `case_sensitive` follow `Graph::new`. Pass NULL for `name`
/// to use an empty name.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_new(
    name: *const c_char,
    case_sensitive: c_int,
) -> *mut rfl_graph {
    clear_last_error();
    let name = if name.is_null() {
        String::new()
    } else {
        match unsafe { CStr::from_ptr(name) }.to_str() {
            Ok(s) => s.to_owned(),
            Err(_) => {
                set_last_error("name is not valid UTF-8");
                return std::ptr::null_mut();
            }
        }
    };
    let graph = Graph::new(&name, case_sensitive != 0, None);
    Box::into_raw(Box::new(rfl_graph { inner: graph }))
}

/// Load a graph from a `GraphExport` JSON document.
///
/// Returns NULL on failure (check `rfl_last_error_message`).
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_load_json(json: *const c_char) -> *mut rfl_graph {
    clear_last_error();
    if json.is_null() {
        set_last_error("json pointer is null");
        return std::ptr::null_mut();
    }
    let s = match unsafe { CStr::from_ptr(json) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("json is not valid UTF-8");
            return std::ptr::null_mut();
        }
    };
    let export: GraphExport = match serde_json::from_str(s) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(format!("graph json parse: {e}"));
            return std::ptr::null_mut();
        }
    };
    Box::into_raw(Box::new(rfl_graph {
        inner: Graph::load(export, None),
    }))
}

/// Free a graph handle. Safe on NULL.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_free(g: *mut rfl_graph) {
    if !g.is_null() {
        drop(unsafe { Box::from_raw(g) });
    }
}

// ─── network ───────────────────────────────────────────────────────────────

/// Opaque handle to a `reflow_network::Network`, wrapped so the C side can
/// share it across threads without touching its internal `Arc<Mutex<_>>`.
pub struct rfl_network {
    net: Arc<Mutex<Network>>,
}

/// Create a new network with `NetworkConfig::default()`.
#[no_mangle]
pub extern "C" fn rfl_network_new() -> *mut rfl_network {
    clear_last_error();
    let net = Network::new(NetworkConfig::default());
    Box::into_raw(Box::new(rfl_network {
        net: Arc::new(Mutex::new(net)),
    }))
}

/// Create a new network from a serialized `NetworkConfig` JSON. Returns
/// NULL on parse error. Unknown fields are rejected.
#[no_mangle]
pub unsafe extern "C" fn rfl_network_new_with_config(
    config_json: *const c_char,
) -> *mut rfl_network {
    clear_last_error();
    if config_json.is_null() {
        set_last_error("config_json is null");
        return std::ptr::null_mut();
    }
    let s = match unsafe { CStr::from_ptr(config_json) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("config_json is not valid UTF-8");
            return std::ptr::null_mut();
        }
    };
    let cfg: NetworkConfig = match serde_json::from_str(s) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(format!("NetworkConfig parse: {e}"));
            return std::ptr::null_mut();
        }
    };
    let net = Network::new(cfg);
    Box::into_raw(Box::new(rfl_network {
        net: Arc::new(Mutex::new(net)),
    }))
}

/// Create a network from an already-loaded graph. The graph is consumed.
#[no_mangle]
pub unsafe extern "C" fn rfl_network_from_graph(g: *mut rfl_graph) -> *mut rfl_network {
    clear_last_error();
    if g.is_null() {
        set_last_error("graph pointer is null");
        return std::ptr::null_mut();
    }
    let graph = unsafe { Box::from_raw(g) }.inner;
    // `Network::with_graph` spawns subgraph worker tasks via
    // `tokio::spawn` during construction, so we must enter the
    // runtime context here. Without this guard the call panics with
    // "there is no reactor running" the moment any internal subgraph
    // setup runs.
    let rt = runtime();
    let _enter = rt.enter();
    let net_arc = Network::with_graph(NetworkConfig::default(), &graph);
    // `Network::with_graph` returns `Arc<Mutex<Network>>` already.
    Box::into_raw(Box::new(rfl_network { net: net_arc }))
}

/// Start the network. Non-blocking; returns immediately after scheduling
/// the actors.
#[no_mangle]
pub unsafe extern "C" fn rfl_network_start(n: *mut rfl_network) -> rfl_status {
    clear_last_error();
    if n.is_null() {
        return rfl_status::NullArg;
    }
    let handle = unsafe { &*n };
    let rt = runtime();
    let _enter = rt.enter();
    match handle.net.lock().unwrap().start() {
        Ok(_) => rfl_status::Ok,
        Err(e) => to_status_runtime(e),
    }
}

/// Signal the network to shut down. Non-blocking.
#[no_mangle]
pub unsafe extern "C" fn rfl_network_shutdown(n: *mut rfl_network) -> rfl_status {
    clear_last_error();
    if n.is_null() {
        return rfl_status::NullArg;
    }
    let handle = unsafe { &*n };
    let rt = runtime();
    let _enter = rt.enter();
    handle.net.lock().unwrap().shutdown();
    rfl_status::Ok
}

/// Free a network handle. Safe on NULL. Implies shutdown.
#[no_mangle]
pub unsafe extern "C" fn rfl_network_free(n: *mut rfl_network) {
    if !n.is_null() {
        let handle = unsafe { Box::from_raw(n) };
        let rt = runtime();
        let _enter = rt.enter();
        handle.net.lock().unwrap().shutdown();
    }
}

// ─── event stream ──────────────────────────────────────────────────────────

/// Opaque handle to a subscriber on the network's event stream. One
/// subscriber per handle — create as many as you need.
pub struct rfl_events {
    rx: flume::Receiver<NetworkEvent>,
}

/// Subscribe to the network's event stream. Call **before**
/// `rfl_network_start` for full coverage.
#[no_mangle]
pub unsafe extern "C" fn rfl_network_events(n: *mut rfl_network) -> *mut rfl_events {
    clear_last_error();
    if n.is_null() {
        set_last_error("network pointer is null");
        return std::ptr::null_mut();
    }
    let handle = unsafe { &*n };
    let rx = handle.net.lock().unwrap().get_event_receiver();
    Box::into_raw(Box::new(rfl_events { rx }))
}

/// Poll for the next event, blocking up to `timeout_ms` milliseconds.
///
/// On success, writes a newly allocated JSON-encoded event string to
/// `*out_json`. Caller frees with `rfl_string_free`.
///
/// Returns:
/// - `rfl_status::Ok` on success.
/// - `rfl_status::Runtime` if the channel is closed (network dropped).
/// - `rfl_status::InvalidState` on timeout (no event, channel still open).
#[no_mangle]
pub unsafe extern "C" fn rfl_events_recv(
    e: *mut rfl_events,
    timeout_ms: u32,
    out_json: *mut *mut c_char,
) -> rfl_status {
    clear_last_error();
    if e.is_null() || out_json.is_null() {
        return rfl_status::NullArg;
    }
    let handle = unsafe { &*e };
    let deadline = std::time::Duration::from_millis(timeout_ms as u64);
    match handle.rx.recv_timeout(deadline) {
        Ok(evt) => match serde_json::to_string(&evt) {
            Ok(json) => match CString::new(json) {
                Ok(cstr) => {
                    unsafe { *out_json = cstr.into_raw() };
                    rfl_status::Ok
                }
                Err(_) => {
                    set_last_error("event json contains a NUL byte");
                    rfl_status::InvalidJson
                }
            },
            Err(e) => to_status_runtime(e),
        },
        Err(flume::RecvTimeoutError::Timeout) => rfl_status::InvalidState,
        Err(flume::RecvTimeoutError::Disconnected) => {
            set_last_error("event channel disconnected");
            rfl_status::Runtime
        }
    }
}

/// Free an events handle. Safe on NULL.
#[no_mangle]
pub unsafe extern "C" fn rfl_events_free(e: *mut rfl_events) {
    if !e.is_null() {
        drop(unsafe { Box::from_raw(e) });
    }
}

// ─── version ───────────────────────────────────────────────────────────────

/// Runtime version string (newly allocated; free with `rfl_string_free`).
#[no_mangle]
pub extern "C" fn rfl_version() -> *mut c_char {
    CString::new(env!("CARGO_PKG_VERSION")).unwrap().into_raw()
}

// ─── tracing ───────────────────────────────────────────────────────────────
//
// Two consumer surfaces, mirroring the `rfl_events` poll pattern:
//   • Local tap — `rfl_network_traces` polls a network's own trace events with
//     no collector required (the ergonomic default for local monitoring).
//   • Collector client — `rfl_trace_client_*` connect/query/subscribe against a
//     shared tracing server for historical and distributed monitoring.
//
// Enabling tracing is done via `NetworkConfig.tracing` in the config JSON passed
// to `rfl_network_new_with_config` (server_url + enabled + capture knobs).

use reflow_tracing_protocol::client::{TracingClient, TracingConfig};
use reflow_tracing_protocol::{SubscriptionFilters, TraceEvent, TraceQuery};

/// Serialize a value to a newly allocated JSON C string in `*out`.
unsafe fn write_json<T: serde::Serialize>(v: &T, out: *mut *mut c_char) -> rfl_status {
    match serde_json::to_string(v) {
        Ok(json) => match CString::new(json) {
            Ok(cstr) => {
                unsafe { *out = cstr.into_raw() };
                rfl_status::Ok
            }
            Err(_) => {
                set_last_error("json contains a NUL byte");
                rfl_status::InvalidJson
            }
        },
        Err(e) => to_status_runtime(e),
    }
}

/// Opaque handle to a subscriber on a network's local trace-event stream.
/// One subscriber per handle.
pub struct rfl_traces {
    rx: flume::Receiver<TraceEvent>,
}

/// Subscribe to a network's live trace events locally — no collector required.
/// Call **before** `rfl_network_start` for full coverage. Returns NULL on a
/// null argument.
#[no_mangle]
pub unsafe extern "C" fn rfl_network_traces(n: *mut rfl_network) -> *mut rfl_traces {
    clear_last_error();
    if n.is_null() {
        set_last_error("network pointer is null");
        return std::ptr::null_mut();
    }
    let handle = unsafe { &*n };
    let rx = handle.net.lock().unwrap().get_trace_receiver();
    Box::into_raw(Box::new(rfl_traces { rx }))
}

/// Poll for the next trace event, blocking up to `timeout_ms` milliseconds.
/// On success writes a newly allocated JSON `TraceEvent` to `*out_json` (free
/// with `rfl_string_free`). Returns `Ok`, `InvalidState` on timeout, or
/// `Runtime` if the channel is closed.
#[no_mangle]
pub unsafe extern "C" fn rfl_traces_recv(
    t: *mut rfl_traces,
    timeout_ms: u32,
    out_json: *mut *mut c_char,
) -> rfl_status {
    clear_last_error();
    if t.is_null() || out_json.is_null() {
        return rfl_status::NullArg;
    }
    let handle = unsafe { &*t };
    let deadline = std::time::Duration::from_millis(timeout_ms as u64);
    match handle.rx.recv_timeout(deadline) {
        Ok(evt) => unsafe { write_json(&evt, out_json) },
        Err(flume::RecvTimeoutError::Timeout) => rfl_status::InvalidState,
        Err(flume::RecvTimeoutError::Disconnected) => {
            set_last_error("trace channel disconnected");
            rfl_status::Runtime
        }
    }
}

/// Free a traces handle. Safe on NULL.
#[no_mangle]
pub unsafe extern "C" fn rfl_traces_free(t: *mut rfl_traces) {
    if !t.is_null() {
        drop(unsafe { Box::from_raw(t) });
    }
}

/// Opaque handle to a tracing-collector client.
pub struct rfl_trace_client {
    client: TracingClient,
    sub_rx: Mutex<Option<flume::Receiver<TraceEvent>>>,
}

/// Connect to a tracing collector at `server_url` (e.g. "ws://127.0.0.1:8080").
/// Returns NULL on failure (see `rfl_last_error_message`).
#[no_mangle]
pub unsafe extern "C" fn rfl_trace_client_connect(
    server_url: *const c_char,
) -> *mut rfl_trace_client {
    clear_last_error();
    let url = match unsafe { cstr_to_str(server_url, "server_url") } {
        Ok(s) => s.to_string(),
        Err(_) => return std::ptr::null_mut(),
    };
    let client = TracingClient::new(TracingConfig {
        server_url: url,
        ..TracingConfig::default()
    });
    let rt = runtime();
    if let Err(e) = rt.block_on(client.connect()) {
        set_last_error(format!("trace client connect: {e}"));
        return std::ptr::null_mut();
    }
    Box::into_raw(Box::new(rfl_trace_client {
        client,
        sub_rx: Mutex::new(None),
    }))
}

/// Query historical traces. `query_json` is a JSON `TraceQuery`, or NULL for
/// "all". Writes a JSON array of `FlowTrace` to `*out_json` (free with
/// `rfl_string_free`).
#[no_mangle]
pub unsafe extern "C" fn rfl_trace_client_query(
    c: *mut rfl_trace_client,
    query_json: *const c_char,
    out_json: *mut *mut c_char,
) -> rfl_status {
    clear_last_error();
    if c.is_null() || out_json.is_null() {
        return rfl_status::NullArg;
    }
    let handle = unsafe { &*c };
    let query = if query_json.is_null() {
        empty_trace_query()
    } else {
        let s = match unsafe { cstr_to_str(query_json, "query_json") } {
            Ok(s) => s,
            Err(e) => return e,
        };
        match serde_json::from_str::<TraceQuery>(s) {
            Ok(q) => q,
            Err(e) => {
                set_last_error(format!("query_json: {e}"));
                return rfl_status::InvalidJson;
            }
        }
    };
    let rt = runtime();
    match rt.block_on(handle.client.query_traces(query)) {
        Ok(traces) => unsafe { write_json(&traces, out_json) },
        Err(e) => to_status_runtime(e),
    }
}

/// Subscribe to live trace events from the collector. `filters_json` is a JSON
/// `SubscriptionFilters`, or NULL for no filtering. After this returns `Ok`,
/// poll events with `rfl_trace_client_recv`.
#[no_mangle]
pub unsafe extern "C" fn rfl_trace_client_subscribe(
    c: *mut rfl_trace_client,
    filters_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if c.is_null() {
        return rfl_status::NullArg;
    }
    let handle = unsafe { &*c };
    let filters = if filters_json.is_null() {
        SubscriptionFilters {
            flow_ids: None,
            actor_ids: None,
            event_types: None,
            status_filter: None,
        }
    } else {
        let s = match unsafe { cstr_to_str(filters_json, "filters_json") } {
            Ok(s) => s,
            Err(e) => return e,
        };
        match serde_json::from_str::<SubscriptionFilters>(s) {
            Ok(f) => f,
            Err(e) => {
                set_last_error(format!("filters_json: {e}"));
                return rfl_status::InvalidJson;
            }
        }
    };
    // Bounded so a consumer that stops polling can't grow this without bound;
    // the notification tap drops on a full buffer (best-effort).
    let (tx, rx) = flume::bounded(4096);
    handle.client.set_notification_tap(tx);
    *handle.sub_rx.lock().unwrap() = Some(rx);
    let rt = runtime();
    match rt.block_on(handle.client.subscribe(filters)) {
        Ok(_) => rfl_status::Ok,
        Err(e) => to_status_runtime(e),
    }
}

/// Poll for the next live trace event after `rfl_trace_client_subscribe`,
/// blocking up to `timeout_ms`. Writes a JSON `TraceEvent` to `*out_json`.
#[no_mangle]
pub unsafe extern "C" fn rfl_trace_client_recv(
    c: *mut rfl_trace_client,
    timeout_ms: u32,
    out_json: *mut *mut c_char,
) -> rfl_status {
    clear_last_error();
    if c.is_null() || out_json.is_null() {
        return rfl_status::NullArg;
    }
    let handle = unsafe { &*c };
    let rx = { handle.sub_rx.lock().unwrap().clone() };
    let Some(rx) = rx else {
        set_last_error("not subscribed; call rfl_trace_client_subscribe first");
        return rfl_status::InvalidState;
    };
    let deadline = std::time::Duration::from_millis(timeout_ms as u64);
    match rx.recv_timeout(deadline) {
        Ok(evt) => unsafe { write_json(&evt, out_json) },
        Err(flume::RecvTimeoutError::Timeout) => rfl_status::InvalidState,
        Err(flume::RecvTimeoutError::Disconnected) => {
            set_last_error("subscription channel disconnected");
            rfl_status::Runtime
        }
    }
}

/// Free a trace-client handle. Safe on NULL.
#[no_mangle]
pub unsafe extern "C" fn rfl_trace_client_free(c: *mut rfl_trace_client) {
    if !c.is_null() {
        drop(unsafe { Box::from_raw(c) });
    }
}

fn empty_trace_query() -> TraceQuery {
    TraceQuery {
        flow_id: None,
        execution_id: None,
        time_range: None,
        status: None,
        actor_filter: None,
        limit: None,
        offset: None,
    }
}

// ─── shared helpers ────────────────────────────────────────────────────────

unsafe fn cstr_to_str<'a>(p: *const c_char, name: &str) -> Result<&'a str, rfl_status> {
    if p.is_null() {
        set_last_error(format!("{name} pointer is null"));
        return Err(rfl_status::NullArg);
    }
    match unsafe { CStr::from_ptr(p) }.to_str() {
        Ok(s) => Ok(s),
        Err(_) => {
            set_last_error(format!("{name} is not valid UTF-8"));
            Err(rfl_status::InvalidUtf8)
        }
    }
}

/// Parse a nullable JSON string into `Option<HashMap<String, Value>>`.
unsafe fn parse_metadata(p: *const c_char) -> Result<Option<HashMap<String, Value>>, rfl_status> {
    if p.is_null() {
        return Ok(None);
    }
    let s = unsafe { cstr_to_str(p, "metadata_json")? };
    match serde_json::from_str::<HashMap<String, Value>>(s) {
        Ok(m) => Ok(Some(m)),
        Err(e) => {
            set_last_error(format!("metadata_json parse: {e}"));
            Err(rfl_status::InvalidJson)
        }
    }
}

unsafe fn parse_message(p: *const c_char, name: &str) -> Result<Message, rfl_status> {
    let s = unsafe { cstr_to_str(p, name)? };
    match serde_json::from_str::<Message>(s) {
        Ok(m) => Ok(m),
        Err(e) => {
            set_last_error(format!("{name} parse (Message): {e}"));
            Err(rfl_status::InvalidJson)
        }
    }
}

// ─── graph builder ─────────────────────────────────────────────────────────

/// Add a node.
/// `metadata_json` may be NULL or a JSON object string (`{"key": ...}`).
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_add_node(
    g: *mut rfl_graph,
    id: *const c_char,
    component: *const c_char,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let id = match unsafe { cstr_to_str(id, "id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let component = match unsafe { cstr_to_str(component, "component") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let metadata = match unsafe { parse_metadata(metadata_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };

    unsafe { &mut *g }.inner.add_node(id, component, metadata);
    rfl_status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_remove_node(g: *mut rfl_graph, id: *const c_char) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let id = match unsafe { cstr_to_str(id, "id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }.inner.remove_node(id);
    rfl_status::Ok
}

/// Replace the metadata for an existing node.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_set_node_metadata(
    g: *mut rfl_graph,
    id: *const c_char,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let id = match unsafe { cstr_to_str(id, "id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let metadata = match unsafe { parse_metadata(metadata_json) } {
        Ok(Some(m)) => m,
        Ok(None) => HashMap::new(),
        Err(s) => return s,
    };
    unsafe { &mut *g }.inner.set_node_metadata(id, metadata);
    rfl_status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_add_connection(
    g: *mut rfl_graph,
    out_node: *const c_char,
    out_port: *const c_char,
    in_node: *const c_char,
    in_port: *const c_char,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let out_node = match unsafe { cstr_to_str(out_node, "out_node") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let out_port = match unsafe { cstr_to_str(out_port, "out_port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let in_node = match unsafe { cstr_to_str(in_node, "in_node") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let in_port = match unsafe { cstr_to_str(in_port, "in_port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let metadata = match unsafe { parse_metadata(metadata_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };

    unsafe { &mut *g }
        .inner
        .add_connection(out_node, out_port, in_node, in_port, metadata);
    rfl_status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_remove_connection(
    g: *mut rfl_graph,
    out_node: *const c_char,
    out_port: *const c_char,
    in_node: *const c_char,
    in_port: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let out_node = match unsafe { cstr_to_str(out_node, "out_node") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let out_port = match unsafe { cstr_to_str(out_port, "out_port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let in_node = match unsafe { cstr_to_str(in_node, "in_node") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let in_port = match unsafe { cstr_to_str(in_port, "in_port") } {
        Ok(v) => v,
        Err(s) => return s,
    };

    unsafe { &mut *g }
        .inner
        .remove_connection(out_node, out_port, in_node, in_port);
    rfl_status::Ok
}

/// Add an initial packet. `data_json` is the JSON representation of the
/// value (not a `Message`) — matches `Graph::add_initial`.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_add_initial(
    g: *mut rfl_graph,
    node: *const c_char,
    port: *const c_char,
    data_json: *const c_char,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let node = match unsafe { cstr_to_str(node, "node") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let port = match unsafe { cstr_to_str(port, "port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let data_s = match unsafe { cstr_to_str(data_json, "data_json") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let data: Value = match serde_json::from_str(data_s) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(format!("data_json parse: {e}"));
            return rfl_status::InvalidJson;
        }
    };
    let metadata = match unsafe { parse_metadata(metadata_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };

    unsafe { &mut *g }
        .inner
        .add_initial(data, node, port, metadata);
    rfl_status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_remove_initial(
    g: *mut rfl_graph,
    node: *const c_char,
    port: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let node = match unsafe { cstr_to_str(node, "node") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let port = match unsafe { cstr_to_str(port, "port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }.inner.remove_initial(node, port);
    rfl_status::Ok
}

/// Expose an inport on the graph (used when this graph is embedded as a
/// subgraph). `port_type_json` is optional; pass NULL for `PortType::Any`
/// or `"\"All\""` / `"\"Flow\""` etc.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_add_inport(
    g: *mut rfl_graph,
    port_id: *const c_char,
    node_id: *const c_char,
    port_key: *const c_char,
    port_type_json: *const c_char,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let port_id = match unsafe { cstr_to_str(port_id, "port_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let node_id = match unsafe { cstr_to_str(node_id, "node_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let port_key = match unsafe { cstr_to_str(port_key, "port_key") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let port_type = match unsafe { parse_port_type(port_type_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let metadata = match unsafe { parse_metadata(metadata_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };

    unsafe { &mut *g }
        .inner
        .add_inport(port_id, node_id, port_key, port_type, metadata);
    rfl_status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_add_outport(
    g: *mut rfl_graph,
    port_id: *const c_char,
    node_id: *const c_char,
    port_key: *const c_char,
    port_type_json: *const c_char,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let port_id = match unsafe { cstr_to_str(port_id, "port_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let node_id = match unsafe { cstr_to_str(node_id, "node_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let port_key = match unsafe { cstr_to_str(port_key, "port_key") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let port_type = match unsafe { parse_port_type(port_type_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let metadata = match unsafe { parse_metadata(metadata_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };

    unsafe { &mut *g }
        .inner
        .add_outport(port_id, node_id, port_key, port_type, metadata);
    rfl_status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_remove_inport(
    g: *mut rfl_graph,
    port_id: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let port_id = match unsafe { cstr_to_str(port_id, "port_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }.inner.remove_inport(port_id);
    rfl_status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_remove_outport(
    g: *mut rfl_graph,
    port_id: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let port_id = match unsafe { cstr_to_str(port_id, "port_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }.inner.remove_outport(port_id);
    rfl_status::Ok
}

/// Serialize a graph back to its `GraphExport` JSON form.
/// Caller frees via `rfl_string_free`.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_to_json(g: *mut rfl_graph) -> *mut c_char {
    clear_last_error();
    if g.is_null() {
        set_last_error("graph pointer is null");
        return std::ptr::null_mut();
    }
    let export = unsafe { &*g }.inner.export();
    match serde_json::to_string(&export) {
        Ok(s) => CString::new(s).map(|c| c.into_raw()).unwrap_or_else(|_| {
            set_last_error("graph json contained a NUL byte");
            std::ptr::null_mut()
        }),
        Err(e) => {
            set_last_error(format!("graph export: {e}"));
            std::ptr::null_mut()
        }
    }
}

unsafe fn parse_port_type(
    p: *const c_char,
) -> Result<reflow_rt::graph::types::PortType, rfl_status> {
    use reflow_rt::graph::types::PortType;
    if p.is_null() {
        return Ok(PortType::Any);
    }
    let s = unsafe { cstr_to_str(p, "port_type_json")? };
    match serde_json::from_str::<PortType>(s) {
        Ok(v) => Ok(v),
        Err(e) => {
            set_last_error(format!("port_type_json parse: {e}"));
            Err(rfl_status::InvalidJson)
        }
    }
}

/// Coerce parse_metadata's `Option<HashMap<...>>` into the
/// `HashMap<...>` shape that `set_*_metadata` and `set_properties`
/// expect (NULL → empty map).
unsafe fn parse_metadata_required(p: *const c_char) -> Result<HashMap<String, Value>, rfl_status> {
    match unsafe { parse_metadata(p) } {
        Ok(Some(m)) => Ok(m),
        Ok(None) => Ok(HashMap::new()),
        Err(s) => Err(s),
    }
}

unsafe fn parse_value_json(p: *const c_char, name: &str) -> Result<Value, rfl_status> {
    let s = unsafe { cstr_to_str(p, name)? };
    match serde_json::from_str::<Value>(s) {
        Ok(v) => Ok(v),
        Err(e) => {
            set_last_error(format!("{name} parse: {e}"));
            Err(rfl_status::InvalidJson)
        }
    }
}

fn json_to_cstring(value: &impl serde::Serialize, what: &str) -> *mut c_char {
    match serde_json::to_string(value) {
        Ok(s) => CString::new(s).map(|c| c.into_raw()).unwrap_or_else(|_| {
            set_last_error(format!("{what} contained a NUL byte"));
            std::ptr::null_mut()
        }),
        Err(e) => {
            set_last_error(format!("{what} serialize: {e}"));
            std::ptr::null_mut()
        }
    }
}

// ─── graph mutators (rename / port-rename) ────────────────────────────────

/// Rename a node. Updates every connection that referenced the old id.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_rename_node(
    g: *mut rfl_graph,
    old_id: *const c_char,
    new_id: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let old = match unsafe { cstr_to_str(old_id, "old_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let new = match unsafe { cstr_to_str(new_id, "new_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }.inner.rename_node(old, new);
    rfl_status::Ok
}

/// Rename an exposed inport (subgraph boundary).
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_rename_inport(
    g: *mut rfl_graph,
    old_port: *const c_char,
    new_port: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let old = match unsafe { cstr_to_str(old_port, "old_port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let new = match unsafe { cstr_to_str(new_port, "new_port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }.inner.rename_inport(old, new);
    rfl_status::Ok
}

/// Rename an exposed outport (subgraph boundary).
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_rename_outport(
    g: *mut rfl_graph,
    old_port: *const c_char,
    new_port: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let old = match unsafe { cstr_to_str(old_port, "old_port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let new = match unsafe { cstr_to_str(new_port, "new_port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }.inner.rename_outport(old, new);
    rfl_status::Ok
}

// ─── graph mutators (initial packets) ──────────────────────────────────────

/// `add_initial` with an explicit slot index for arrays/streams. Mirrors
/// `Graph::add_initial_index`.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_add_initial_index(
    g: *mut rfl_graph,
    node: *const c_char,
    port: *const c_char,
    data_json: *const c_char,
    index: usize,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let node = match unsafe { cstr_to_str(node, "node") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let port = match unsafe { cstr_to_str(port, "port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let data = match unsafe { parse_value_json(data_json, "data_json") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let metadata = match unsafe { parse_metadata(metadata_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }
        .inner
        .add_initial_index(data, node, port, index, metadata);
    rfl_status::Ok
}

/// Push a packet into one of the graph's exposed inports.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_add_graph_initial(
    g: *mut rfl_graph,
    inport: *const c_char,
    data_json: *const c_char,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let inport = match unsafe { cstr_to_str(inport, "inport") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let data = match unsafe { parse_value_json(data_json, "data_json") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let metadata = match unsafe { parse_metadata(metadata_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }
        .inner
        .add_graph_initial(data, inport, metadata);
    rfl_status::Ok
}

/// Indexed variant of `rfl_graph_add_graph_initial`.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_add_graph_initial_index(
    g: *mut rfl_graph,
    inport: *const c_char,
    data_json: *const c_char,
    index: usize,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let inport = match unsafe { cstr_to_str(inport, "inport") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let data = match unsafe { parse_value_json(data_json, "data_json") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let metadata = match unsafe { parse_metadata(metadata_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }
        .inner
        .add_graph_initial_index(data, inport, index, metadata);
    rfl_status::Ok
}

/// Remove an initial packet attached to a graph-level inport.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_remove_graph_initial(
    g: *mut rfl_graph,
    inport: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let inport = match unsafe { cstr_to_str(inport, "inport") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }.inner.remove_graph_initial(inport);
    rfl_status::Ok
}

// ─── graph mutators (groups) ───────────────────────────────────────────────

/// Create a group containing the given node ids.
/// `nodes_json` must be a JSON array of strings, e.g. `["a","b"]`.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_add_group(
    g: *mut rfl_graph,
    group_id: *const c_char,
    nodes_json: *const c_char,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let group_id = match unsafe { cstr_to_str(group_id, "group_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let nodes_s = match unsafe { cstr_to_str(nodes_json, "nodes_json") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let nodes: Vec<String> = match serde_json::from_str(nodes_s) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(format!("nodes_json parse: {e}"));
            return rfl_status::InvalidJson;
        }
    };
    let metadata = match unsafe { parse_metadata(metadata_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }
        .inner
        .add_group(group_id, nodes, metadata);
    rfl_status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_remove_group(
    g: *mut rfl_graph,
    group_id: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let group_id = match unsafe { cstr_to_str(group_id, "group_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }.inner.remove_group(group_id);
    rfl_status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_add_to_group(
    g: *mut rfl_graph,
    group_id: *const c_char,
    node_id: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let group_id = match unsafe { cstr_to_str(group_id, "group_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let node_id = match unsafe { cstr_to_str(node_id, "node_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }.inner.add_to_group(group_id, node_id);
    rfl_status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_remove_from_group(
    g: *mut rfl_graph,
    group_id: *const c_char,
    node_id: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let group_id = match unsafe { cstr_to_str(group_id, "group_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let node_id = match unsafe { cstr_to_str(node_id, "node_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }
        .inner
        .remove_from_group(group_id, node_id);
    rfl_status::Ok
}

// ─── graph mutators (metadata) ─────────────────────────────────────────────

/// Replace the metadata on a connection. NULL `metadata_json` clears it.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_set_connection_metadata(
    g: *mut rfl_graph,
    out_node: *const c_char,
    out_port: *const c_char,
    in_node: *const c_char,
    in_port: *const c_char,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let out_node = match unsafe { cstr_to_str(out_node, "out_node") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let out_port = match unsafe { cstr_to_str(out_port, "out_port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let in_node = match unsafe { cstr_to_str(in_node, "in_node") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let in_port = match unsafe { cstr_to_str(in_port, "in_port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let metadata = match unsafe { parse_metadata_required(metadata_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }
        .inner
        .set_connection_metadata(out_node, out_port, in_node, in_port, metadata);
    rfl_status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_set_inport_metadata(
    g: *mut rfl_graph,
    port_id: *const c_char,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let port_id = match unsafe { cstr_to_str(port_id, "port_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let metadata = match unsafe { parse_metadata_required(metadata_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }
        .inner
        .set_inport_metadata(port_id, metadata);
    rfl_status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_set_outport_metadata(
    g: *mut rfl_graph,
    port_id: *const c_char,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let port_id = match unsafe { cstr_to_str(port_id, "port_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let metadata = match unsafe { parse_metadata_required(metadata_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }
        .inner
        .set_outport_metadata(port_id, metadata);
    rfl_status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_set_group_metadata(
    g: *mut rfl_graph,
    group_id: *const c_char,
    metadata_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let group_id = match unsafe { cstr_to_str(group_id, "group_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let metadata = match unsafe { parse_metadata_required(metadata_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }
        .inner
        .set_group_metadata(group_id, metadata);
    rfl_status::Ok
}

/// Replace the graph's properties dict. NULL clears it.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_set_properties(
    g: *mut rfl_graph,
    properties_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let props = match unsafe { parse_metadata_required(properties_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    unsafe { &mut *g }.inner.set_properties(props);
    rfl_status::Ok
}

/// Merge a `GraphExport` JSON document into the existing graph
/// (additive — does not clear).
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_import(
    g: *mut rfl_graph,
    export_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if g.is_null() {
        return rfl_status::NullArg;
    }
    let s = match unsafe { cstr_to_str(export_json, "export_json") } {
        Ok(v) => v,
        Err(st) => return st,
    };
    let export: GraphExport = match serde_json::from_str(s) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(format!("export_json parse: {e}"));
            return rfl_status::InvalidJson;
        }
    };
    unsafe { &mut *g }.inner.import(export);
    rfl_status::Ok
}

// ─── graph queries (return JSON, caller frees with rfl_string_free) ───────

/// Look up a node by id. Returns the JSON of the GraphNode, or NULL if
/// the id is unknown (last error explains).
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_get_node_json(
    g: *mut rfl_graph,
    id: *const c_char,
) -> *mut c_char {
    clear_last_error();
    if g.is_null() {
        set_last_error("graph pointer is null");
        return std::ptr::null_mut();
    }
    let id = match unsafe { cstr_to_str(id, "id") } {
        Ok(v) => v,
        Err(_) => return std::ptr::null_mut(),
    };
    match unsafe { &*g }.inner.get_node(id) {
        Some(n) => json_to_cstring(n, "node"),
        None => {
            set_last_error(format!("no node with id '{id}'"));
            std::ptr::null_mut()
        }
    }
}

/// JSON array of every node in the graph.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_list_nodes_json(g: *mut rfl_graph) -> *mut c_char {
    clear_last_error();
    if g.is_null() {
        set_last_error("graph pointer is null");
        return std::ptr::null_mut();
    }
    let nodes = unsafe { &*g }.inner.get_nodes();
    json_to_cstring(&nodes, "nodes")
}

/// Look up a connection by both endpoints. NULL if no such edge.
#[no_mangle]
pub unsafe extern "C" fn rfl_graph_get_connection_json(
    g: *mut rfl_graph,
    out_node: *const c_char,
    out_port: *const c_char,
    in_node: *const c_char,
    in_port: *const c_char,
) -> *mut c_char {
    clear_last_error();
    if g.is_null() {
        set_last_error("graph pointer is null");
        return std::ptr::null_mut();
    }
    let out_node = match unsafe { cstr_to_str(out_node, "out_node") } {
        Ok(v) => v,
        Err(_) => return std::ptr::null_mut(),
    };
    let out_port = match unsafe { cstr_to_str(out_port, "out_port") } {
        Ok(v) => v,
        Err(_) => return std::ptr::null_mut(),
    };
    let in_node = match unsafe { cstr_to_str(in_node, "in_node") } {
        Ok(v) => v,
        Err(_) => return std::ptr::null_mut(),
    };
    let in_port = match unsafe { cstr_to_str(in_port, "in_port") } {
        Ok(v) => v,
        Err(_) => return std::ptr::null_mut(),
    };
    match unsafe { &*g }
        .inner
        .get_connection(out_node, out_port, in_node, in_port)
    {
        Some(c) => json_to_cstring(&c, "connection"),
        None => {
            set_last_error(format!(
                "no connection {out_node}.{out_port} -> {in_node}.{in_port}"
            ));
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_list_connections_json(g: *mut rfl_graph) -> *mut c_char {
    clear_last_error();
    if g.is_null() {
        set_last_error("graph pointer is null");
        return std::ptr::null_mut();
    }
    let conns = unsafe { &*g }.inner.get_connections();
    json_to_cstring(&conns, "connections")
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_list_groups_json(g: *mut rfl_graph) -> *mut c_char {
    clear_last_error();
    if g.is_null() {
        set_last_error("graph pointer is null");
        return std::ptr::null_mut();
    }
    let groups = &unsafe { &*g }.inner.groups;
    json_to_cstring(groups, "groups")
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_list_inports_json(g: *mut rfl_graph) -> *mut c_char {
    clear_last_error();
    if g.is_null() {
        set_last_error("graph pointer is null");
        return std::ptr::null_mut();
    }
    // `inports` is `pub(crate)`. Round-trip through `export()` to read it.
    let export = unsafe { &*g }.inner.export();
    json_to_cstring(&export.inports, "inports")
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_list_outports_json(g: *mut rfl_graph) -> *mut c_char {
    clear_last_error();
    if g.is_null() {
        set_last_error("graph pointer is null");
        return std::ptr::null_mut();
    }
    let export = unsafe { &*g }.inner.export();
    json_to_cstring(&export.outports, "outports")
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_list_initializers_json(g: *mut rfl_graph) -> *mut c_char {
    clear_last_error();
    if g.is_null() {
        set_last_error("graph pointer is null");
        return std::ptr::null_mut();
    }
    let inits = &unsafe { &*g }.inner.initializers;
    json_to_cstring(inits, "initializers")
}

#[no_mangle]
pub unsafe extern "C" fn rfl_graph_get_properties_json(g: *mut rfl_graph) -> *mut c_char {
    clear_last_error();
    if g.is_null() {
        set_last_error("graph pointer is null");
        return std::ptr::null_mut();
    }
    let props = unsafe { &*g }.inner.get_properties();
    json_to_cstring(&props, "properties")
}

// ─── network builder ───────────────────────────────────────────────────────

/// Add a node to a running or pending network. The `template_id`'s actor
/// must have been registered first (via the component catalog or a custom
/// `rfl_actor_register` once available).
#[no_mangle]
pub unsafe extern "C" fn rfl_network_add_node(
    n: *mut rfl_network,
    id: *const c_char,
    template_id: *const c_char,
    config_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if n.is_null() {
        return rfl_status::NullArg;
    }
    let id = match unsafe { cstr_to_str(id, "id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let template_id = match unsafe { cstr_to_str(template_id, "template_id") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let metadata = match unsafe { parse_metadata(config_json) } {
        Ok(v) => v,
        Err(s) => return s,
    };

    match unsafe { &*n }
        .net
        .lock()
        .unwrap()
        .add_node(id, template_id, metadata)
    {
        Ok(_) => rfl_status::Ok,
        Err(e) => to_status_runtime(e),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfl_network_add_connection(
    n: *mut rfl_network,
    from_actor: *const c_char,
    from_port: *const c_char,
    to_actor: *const c_char,
    to_port: *const c_char,
) -> rfl_status {
    clear_last_error();
    if n.is_null() {
        return rfl_status::NullArg;
    }
    let from_actor = match unsafe { cstr_to_str(from_actor, "from_actor") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let from_port = match unsafe { cstr_to_str(from_port, "from_port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let to_actor = match unsafe { cstr_to_str(to_actor, "to_actor") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let to_port = match unsafe { cstr_to_str(to_port, "to_port") } {
        Ok(v) => v,
        Err(s) => return s,
    };

    let conn = Connector {
        from: ConnectionPoint {
            actor: from_actor.into(),
            port: from_port.into(),
            initial_data: None,
        },
        to: ConnectionPoint {
            actor: to_actor.into(),
            port: to_port.into(),
            initial_data: None,
        },
    };
    unsafe { &*n }.net.lock().unwrap().add_connection(conn);
    rfl_status::Ok
}

/// Seed an initial packet. `message_json` must parse as a `Message`
/// (e.g. `"\"Flow\""`, `{"Integer": 3}`, `{"String": "hi"}`).
#[no_mangle]
pub unsafe extern "C" fn rfl_network_add_initial(
    n: *mut rfl_network,
    actor: *const c_char,
    port: *const c_char,
    message_json: *const c_char,
) -> rfl_status {
    clear_last_error();
    if n.is_null() {
        return rfl_status::NullArg;
    }
    let actor = match unsafe { cstr_to_str(actor, "actor") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let port = match unsafe { cstr_to_str(port, "port") } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let msg = match unsafe { parse_message(message_json, "message_json") } {
        Ok(v) => v,
        Err(s) => return s,
    };

    let iip = InitialPacket {
        to: ConnectionPoint::new(actor, port, Some(msg)),
    };
    unsafe { &*n }.net.lock().unwrap().add_initial(iip);
    rfl_status::Ok
}
