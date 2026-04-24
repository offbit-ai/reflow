//! Callback-driven actor bridge.
//!
//! Lets a non-Rust caller register a function that implements an actor.
//! The `Actor` trait impl (`CapiActor`) owns a `Arc<CapiCallback>` that
//! wraps the raw callback + user-data pointer, firing the user-supplied
//! drop hook exactly once when the last `Arc` reference dies.
//!
//! Threading: callbacks run on tokio worker threads. Language SDKs are
//! expected to handle their own synchronization (GIL, ThreadSafeFunction,
//! `JNIEnv::attach_current_thread`, etc.).

#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

use std::collections::HashMap;
use std::ffi::{c_void, CStr, CString};
use std::future::Future;
use std::os::raw::c_char;
use std::pin::Pin;
use std::sync::Arc;

use parking_lot::Mutex;
use reflow_rt::actor_runtime::message::Message;
use reflow_rt::actor_runtime::{
    Actor, ActorBehavior, ActorConfig, ActorContext, ActorLoad, ActorState, MemoryState, Port,
};

use crate::{rfl_network, rfl_status, set_last_error};

// ─── callback ABI ──────────────────────────────────────────────────────────

/// Opaque per-call context handed to a callback actor.
pub struct rfl_actor_ctx {
    payload: HashMap<String, Message>,
    config: ActorConfig,
    state: Arc<Mutex<dyn ActorState>>,
    outputs: Mutex<HashMap<String, Message>>,
}

/// Function pointer: the body of a callback actor.
///
/// The callback is invoked every time the runtime has inputs for the actor.
/// Return `rfl_status::Ok` to commit whatever was emitted via
/// `rfl_ctx_emit`; any other status aborts the tick with an error in the
/// network event stream.
pub type rfl_actor_fn =
    Option<unsafe extern "C" fn(user_data: *mut c_void, ctx: *mut rfl_actor_ctx) -> rfl_status>;

/// Function pointer: released when the runtime drops the last reference to
/// the actor. Use it to decrement a Node/Python/JVM GC root.
pub type rfl_actor_drop_fn = Option<unsafe extern "C" fn(user_data: *mut c_void)>;

// ─── CapiCallback — shared immutable callback descriptor ───────────────────

struct SendPtr(*mut c_void);
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

struct CapiCallback {
    callback: rfl_actor_fn,
    user_data: SendPtr,
    user_data_drop: rfl_actor_drop_fn,
    component: String,
    inports: Vec<String>,
    outports: Vec<String>,
    await_all_inports: bool,
}

impl Drop for CapiCallback {
    fn drop(&mut self) {
        if let Some(drop_fn) = self.user_data_drop {
            unsafe { drop_fn(self.user_data.0) };
        }
    }
}

// ─── CapiActor — per-instance Actor impl ───────────────────────────────────

pub struct CapiActor {
    cb: Arc<CapiCallback>,
    inports: Port,
    outports: Port,
    load: Arc<ActorLoad>,
}

impl CapiActor {
    fn from_callback(cb: Arc<CapiCallback>) -> Self {
        Self {
            cb,
            inports: flume::bounded(50),
            outports: flume::bounded(50),
            load: Arc::new(ActorLoad::new(0)),
        }
    }
}

impl Actor for CapiActor {
    #[allow(clippy::type_complexity)]
    fn get_behavior(&self) -> ActorBehavior {
        let cb = Arc::clone(&self.cb);
        Box::new(
            move |ctx: ActorContext| -> Pin<
                Box<
                    dyn Future<Output = Result<HashMap<String, Message>, anyhow::Error>>
                        + Send
                        + 'static,
                >,
            > {
                let cb = Arc::clone(&cb);
                Box::pin(async move {
                    let payload = ctx.get_payload().clone();
                    let config = ctx.get_config().clone();
                    let state = ctx.get_state();

                    let mut capi_ctx = Box::new(rfl_actor_ctx {
                        payload,
                        config,
                        state,
                        outputs: Mutex::new(HashMap::new()),
                    });
                    let ctx_ptr: *mut rfl_actor_ctx = capi_ctx.as_mut();

                    let status = match cb.callback {
                        Some(f) => unsafe { f(cb.user_data.0, ctx_ptr) },
                        None => rfl_status::Runtime,
                    };

                    match status {
                        rfl_status::Ok => {
                            let outputs = std::mem::take(&mut *capi_ctx.outputs.lock());
                            Ok(outputs)
                        }
                        other => Err(anyhow::anyhow!(
                            "capi actor '{}' callback returned status {:?}",
                            cb.component,
                            other
                        )),
                    }
                })
            },
        )
    }

    fn get_outports(&self) -> Port {
        self.outports.clone()
    }

    fn get_inports(&self) -> Port {
        self.inports.clone()
    }

    fn inport_names(&self) -> Vec<String> {
        self.cb.inports.clone()
    }

    fn outport_names(&self) -> Vec<String> {
        self.cb.outports.clone()
    }

    fn await_all_inports(&self) -> bool {
        self.cb.await_all_inports
    }

    fn create_state(&self) -> Arc<Mutex<dyn ActorState>> {
        Arc::new(Mutex::new(MemoryState::default()))
    }

    fn load_count(&self) -> Arc<ActorLoad> {
        Arc::clone(&self.load)
    }

    fn create_instance(&self) -> Arc<dyn Actor> {
        Arc::new(CapiActor::from_callback(Arc::clone(&self.cb)))
    }
}

// ─── C ABI: actor handle + registration ────────────────────────────────────

/// Opaque handle to an actor template. Produced by `rfl_actor_new`
/// (callback-driven actor), `rfl_template_actor_new` (bundled component),
/// or `rfl_subgraph_actor_new_from_json` (embedded subgraph). Hand to
/// `rfl_network_register_actor` to publish under a template id, or to
/// `rfl_actor_free` to release without registering.
pub struct rfl_actor {
    pub(crate) inner: Arc<dyn Actor>,
}

/// Create a callback-driven actor.
///
/// `inports` / `outports` arrays point to `n_*` C strings each (UTF-8).
/// `await_all_inports` = 1 makes the runtime buffer packets until every
/// declared inport has data; 0 fires on any input.
///
/// `user_data_drop` may be NULL if no cleanup is needed.
#[no_mangle]
pub unsafe extern "C" fn rfl_actor_new(
    component_name: *const c_char,
    inports: *const *const c_char,
    n_inports: usize,
    outports: *const *const c_char,
    n_outports: usize,
    await_all_inports: std::os::raw::c_int,
    callback: rfl_actor_fn,
    user_data: *mut c_void,
    user_data_drop: rfl_actor_drop_fn,
) -> *mut rfl_actor {
    crate::clear_last_error();
    if component_name.is_null() || callback.is_none() {
        set_last_error("component_name or callback is null");
        return std::ptr::null_mut();
    }
    let component = match unsafe { CStr::from_ptr(component_name) }.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => {
            set_last_error("component_name is not valid UTF-8");
            return std::ptr::null_mut();
        }
    };
    let in_names = match unsafe { cstr_array(inports, n_inports) } {
        Ok(v) => v,
        Err(()) => return std::ptr::null_mut(),
    };
    let out_names = match unsafe { cstr_array(outports, n_outports) } {
        Ok(v) => v,
        Err(()) => return std::ptr::null_mut(),
    };

    let cb = Arc::new(CapiCallback {
        callback,
        user_data: SendPtr(user_data),
        user_data_drop,
        component,
        inports: in_names,
        outports: out_names,
        await_all_inports: await_all_inports != 0,
    });
    Box::into_raw(Box::new(rfl_actor {
        inner: Arc::new(CapiActor::from_callback(cb)) as Arc<dyn Actor>,
    }))
}

/// Free an actor handle that was never registered. NULL-safe.
#[no_mangle]
pub unsafe extern "C" fn rfl_actor_free(a: *mut rfl_actor) {
    if !a.is_null() {
        drop(unsafe { Box::from_raw(a) });
    }
}

/// Register a callback actor as a template on the network. The network
/// takes ownership; the caller must **not** continue to use the handle
/// afterwards (treat it as freed).
#[no_mangle]
pub unsafe extern "C" fn rfl_network_register_actor(
    n: *mut rfl_network,
    template_id: *const c_char,
    a: *mut rfl_actor,
) -> rfl_status {
    crate::clear_last_error();
    if n.is_null() || a.is_null() {
        return rfl_status::NullArg;
    }
    let template_id = match unsafe { CStr::from_ptr(template_id) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("template_id is not valid UTF-8");
            return rfl_status::InvalidUtf8;
        }
    };

    let actor = unsafe { Box::from_raw(a) }.inner;
    let net = unsafe { &*n };
    match net
        .net
        .lock()
        .unwrap()
        .register_actor_arc(template_id, actor)
    {
        Ok(_) => rfl_status::Ok,
        Err(e) => {
            set_last_error(format!("{e}"));
            rfl_status::Runtime
        }
    }
}

unsafe fn cstr_array(ptr: *const *const c_char, n: usize) -> Result<Vec<String>, ()> {
    if n == 0 {
        return Ok(Vec::new());
    }
    if ptr.is_null() {
        set_last_error("string-array pointer is null while count > 0");
        return Err(());
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, n) };
    let mut out = Vec::with_capacity(n);
    for (i, item) in slice.iter().enumerate() {
        if item.is_null() {
            set_last_error(format!("array entry {i} is null"));
            return Err(());
        }
        match unsafe { CStr::from_ptr(*item) }.to_str() {
            Ok(s) => out.push(s.to_owned()),
            Err(_) => {
                set_last_error(format!("array entry {i} is not valid UTF-8"));
                return Err(());
            }
        }
    }
    Ok(out)
}

// ─── C ABI: callback context accessors ─────────────────────────────────────

/// 1 if a packet is available on `port`, 0 otherwise. Does not consume.
#[no_mangle]
pub unsafe extern "C" fn rfl_ctx_has_input(
    ctx: *mut rfl_actor_ctx,
    port: *const c_char,
) -> std::os::raw::c_int {
    if ctx.is_null() || port.is_null() {
        return 0;
    }
    let port = match unsafe { CStr::from_ptr(port) }.to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };
    if unsafe { &*ctx }.payload.contains_key(port) {
        1
    } else {
        0
    }
}

/// Take the input packet on `port` as a typed message handle, removing
/// it from the context. Returns NULL if no packet is available. Caller
/// frees via `rfl_message_free` (or transfers ownership via
/// `rfl_ctx_emit_message`).
#[no_mangle]
pub unsafe extern "C" fn rfl_ctx_take_input_message(
    ctx: *mut rfl_actor_ctx,
    port: *const c_char,
) -> *mut crate::message::rfl_message {
    crate::clear_last_error();
    if ctx.is_null() || port.is_null() {
        return std::ptr::null_mut();
    }
    let port = match unsafe { CStr::from_ptr(port) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    // SAFETY: payload is only accessed from the single callback thread.
    let payload = &mut unsafe { &mut *ctx }.payload;
    match payload.remove(port) {
        Some(m) => Box::into_raw(Box::new(crate::message::rfl_message { inner: m })),
        None => std::ptr::null_mut(),
    }
}

/// Return the input packet on `port` as a JSON-encoded `Message`, or NULL
/// if no packet is available. Caller frees via `rfl_string_free`.
#[no_mangle]
pub unsafe extern "C" fn rfl_ctx_input_json(
    ctx: *mut rfl_actor_ctx,
    port: *const c_char,
) -> *mut c_char {
    crate::clear_last_error();
    if ctx.is_null() || port.is_null() {
        return std::ptr::null_mut();
    }
    let port = match unsafe { CStr::from_ptr(port) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let payload = &unsafe { &*ctx }.payload;
    match payload.get(port) {
        Some(m) => match serde_json::to_string(m) {
            Ok(s) => CString::new(s)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            Err(e) => {
                set_last_error(format!("message serialize: {e}"));
                std::ptr::null_mut()
            }
        },
        None => std::ptr::null_mut(),
    }
}

/// Return the current config as a JSON object string. Caller frees via
/// `rfl_string_free`. Returns NULL on error.
#[no_mangle]
pub unsafe extern "C" fn rfl_ctx_config_json(ctx: *mut rfl_actor_ctx) -> *mut c_char {
    crate::clear_last_error();
    if ctx.is_null() {
        return std::ptr::null_mut();
    }
    let cfg_map = unsafe { &*ctx }.config.as_hashmap();
    match serde_json::to_string(&cfg_map) {
        Ok(s) => CString::new(s)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(e) => {
            set_last_error(format!("config serialize: {e}"));
            std::ptr::null_mut()
        }
    }
}

/// Fetch a state entry as JSON. Returns NULL if absent.
/// Caller frees via `rfl_string_free`.
///
/// Only works against `MemoryState` (the default state backend) — custom
/// backends will yield NULL.
#[no_mangle]
pub unsafe extern "C" fn rfl_ctx_state_get(
    ctx: *mut rfl_actor_ctx,
    key: *const c_char,
) -> *mut c_char {
    crate::clear_last_error();
    if ctx.is_null() || key.is_null() {
        return std::ptr::null_mut();
    }
    let key = match unsafe { CStr::from_ptr(key) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let state = Arc::clone(&unsafe { &*ctx }.state);
    let guard = state.lock();
    let any_ref = guard.as_any();
    let mem = match any_ref.downcast_ref::<MemoryState>() {
        Some(m) => m,
        None => return std::ptr::null_mut(),
    };
    match mem.0.get(key) {
        Some(v) => match serde_json::to_string(v) {
            Ok(s) => CString::new(s)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            Err(_) => std::ptr::null_mut(),
        },
        None => std::ptr::null_mut(),
    }
}

/// Set a state entry. `value_json` is a JSON value (any shape).
#[no_mangle]
pub unsafe extern "C" fn rfl_ctx_state_set(
    ctx: *mut rfl_actor_ctx,
    key: *const c_char,
    value_json: *const c_char,
) -> rfl_status {
    crate::clear_last_error();
    if ctx.is_null() || key.is_null() || value_json.is_null() {
        return rfl_status::NullArg;
    }
    let key = match unsafe { CStr::from_ptr(key) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("key is not valid UTF-8");
            return rfl_status::InvalidUtf8;
        }
    };
    let value_s = match unsafe { CStr::from_ptr(value_json) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("value_json is not valid UTF-8");
            return rfl_status::InvalidUtf8;
        }
    };
    let value: serde_json::Value = match serde_json::from_str(value_s) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(format!("value_json parse: {e}"));
            return rfl_status::InvalidJson;
        }
    };

    let state = Arc::clone(&unsafe { &*ctx }.state);
    let mut guard = state.lock();
    let any_ref = guard.as_mut_any();
    match any_ref.downcast_mut::<MemoryState>() {
        Some(mem) => {
            mem.0.insert(key.to_string(), value);
            rfl_status::Ok
        }
        None => {
            set_last_error("actor state is not a MemoryState; cannot set key from C");
            rfl_status::InvalidState
        }
    }
}

/// Emit a typed message on `port`. Transfers ownership of the message —
/// do **not** call `rfl_message_free` afterwards. Prefer this over the
/// JSON variant for hot-path emits.
#[no_mangle]
pub unsafe extern "C" fn rfl_ctx_emit_message(
    ctx: *mut rfl_actor_ctx,
    port: *const c_char,
    msg: *mut crate::message::rfl_message,
) -> rfl_status {
    crate::clear_last_error();
    if ctx.is_null() || port.is_null() || msg.is_null() {
        return rfl_status::NullArg;
    }
    let port = match unsafe { CStr::from_ptr(port) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("port is not valid UTF-8");
            return rfl_status::InvalidUtf8;
        }
    };
    let msg = unsafe { Box::from_raw(msg) }.into_message();
    unsafe { &*ctx }
        .outputs
        .lock()
        .insert(port.to_string(), msg);
    rfl_status::Ok
}

/// Emit a packet on `port`. `message_json` must parse as a Reflow
/// `Message` (e.g. `{"type":"Flow"}`, `{"type":"Integer","data":1}`).
/// Prefer `rfl_ctx_emit_message` for hot-path emits — this variant
/// serializes JSON per call.
#[no_mangle]
pub unsafe extern "C" fn rfl_ctx_emit(
    ctx: *mut rfl_actor_ctx,
    port: *const c_char,
    message_json: *const c_char,
) -> rfl_status {
    crate::clear_last_error();
    if ctx.is_null() || port.is_null() || message_json.is_null() {
        return rfl_status::NullArg;
    }
    let port = match unsafe { CStr::from_ptr(port) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("port is not valid UTF-8");
            return rfl_status::InvalidUtf8;
        }
    };
    let msg_s = match unsafe { CStr::from_ptr(message_json) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("message_json is not valid UTF-8");
            return rfl_status::InvalidUtf8;
        }
    };
    let msg: Message = match serde_json::from_str(msg_s) {
        Ok(m) => m,
        Err(e) => {
            set_last_error(format!("message_json parse (Message): {e}"));
            return rfl_status::InvalidJson;
        }
    };

    unsafe { &*ctx }
        .outputs
        .lock()
        .insert(port.to_string(), msg);
    rfl_status::Ok
}
