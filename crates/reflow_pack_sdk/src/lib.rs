//! Author-facing crate for Reflow actor packs.
//!
//! ```ignore
//! use reflow_pack_sdk::{reflow_pack, PackHost};
//! use std::sync::Arc;
//!
//! struct Hello;
//! impl reflow_pack_sdk::Actor for Hello { /* … */ }
//!
//! #[reflow_pack]
//! fn register(host: &mut PackHost) {
//!     host.register("my.pack.hello", || Arc::new(Hello));
//! }
//! ```
//!
//! A pack crate needs exactly one dep (`reflow_pack_sdk`), sets `crate-type =
//! ["cdylib"]`, and is built with the **same rustc version** as the host
//! runtime it targets.

#![allow(clippy::missing_safety_doc)]

pub use reflow_pack_macro::reflow_pack;

use std::ffi::{CString, c_void};
use std::sync::Arc;

pub use reflow_actor::message::Message;
pub use reflow_actor::{Actor, ActorContext};
pub use reflow_pack_loader::REFLOW_PACK_ABI_VERSION;
pub use reflow_pack_loader::host::{
    PackActorHandle, PackFactoryDropFn, PackFactoryFn, PackHostVtable, PackRegisterStatus,
};

/// Safe wrapper handed to user code by the macro-emitted entrypoint. It
/// buffers registration calls and reports the final status back.
///
/// On native, `PackHost` borrows a `PackHostVtable` filled by the
/// dlopen-style loader. On wasm32, the same name is bound to the
/// browser ABI's `WasmPackHost` (see the `wasm_abi` module below);
/// the lifetime parameter is preserved as a `PhantomData` so the
/// user's `fn register(host: &mut PackHost)` signature compiles
/// unchanged on both targets.
#[cfg(not(target_arch = "wasm32"))]
pub struct PackHost<'a> {
    vtable: &'a mut PackHostVtable,
    status: i32,
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a> PackHost<'a> {
    /// Construct from the raw vtable passed in by the host. Called by the
    /// `#[reflow_pack]` expansion — users do not call this directly.
    #[doc(hidden)]
    pub fn from_vtable(vtable: &'a mut PackHostVtable) -> Self {
        Self {
            vtable,
            status: PackRegisterStatus::Ok as i32,
        }
    }

    #[doc(hidden)]
    pub fn take_status(self) -> i32 {
        self.status
    }

    /// Register a template id against a factory closure. The closure is
    /// called every time the runtime instantiates a node with this
    /// template id.
    pub fn register<F>(&mut self, template_id: &str, factory: F)
    where
        F: Fn() -> Arc<dyn Actor> + Send + Sync + 'static,
    {
        let id = match CString::new(template_id) {
            Ok(c) => c,
            Err(_) => {
                self.status = PackRegisterStatus::BadUtf8 as i32;
                return;
            }
        };

        // Move factory into a heap allocation we own. The host calls
        // `factory_trampoline` with this pointer on every instantiation,
        // and `factory_drop_trampoline` exactly once when the pack is
        // unloaded.
        let boxed: Box<dyn Fn() -> Arc<dyn Actor> + Send + Sync> = Box::new(factory);
        let user_data = Box::into_raw(Box::new(boxed)) as *mut c_void;

        let status = unsafe {
            (self.vtable.register_template)(
                self.vtable.host_data,
                id.as_ptr(),
                Some(factory_trampoline),
                Some(factory_drop_trampoline),
                user_data,
            )
        };
        if status != PackRegisterStatus::Ok as i32 {
            // First failure is sticky — rest of the pack's registrations
            // will proceed but the pack will still report this code.
            if self.status == PackRegisterStatus::Ok as i32 {
                self.status = status;
            }
            // Reclaim user_data so the failed registration doesn't leak.
            unsafe {
                let _ =
                    Box::from_raw(user_data as *mut Box<dyn Fn() -> Arc<dyn Actor> + Send + Sync>);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
unsafe extern "C" fn factory_trampoline(user_data: *mut c_void) -> *mut PackActorHandle {
    if user_data.is_null() {
        return std::ptr::null_mut();
    }
    let factory = unsafe { &*(user_data as *const Box<dyn Fn() -> Arc<dyn Actor> + Send + Sync>) };
    let actor = factory();
    PackActorHandle::new(actor)
}

#[cfg(not(target_arch = "wasm32"))]
unsafe extern "C" fn factory_drop_trampoline(user_data: *mut c_void) {
    if user_data.is_null() {
        return;
    }
    unsafe {
        let _ = Box::from_raw(user_data as *mut Box<dyn Fn() -> Arc<dyn Actor> + Send + Sync>);
    }
}

pub use reflow_actor::{ActorBehavior, ActorLoad, ActorPayload, ActorState, MemoryState, Port};

// ───────────────────────────────────────────────────────────────────────────
// Wasm pack ABI
// ───────────────────────────────────────────────────────────────────────────
//
// The browser-side pack loader:
//
// 1. Provides a JS function as the pack wasm's
//    `__reflow_pack_register_template` import. JS receives a UTF-8
//    name pointer + length and a factory id; it stores
//    `(name, factory_id)` against the loaded pack instance.
//
// 2. Calls `instance.exports.__reflow_pack_register()` to walk the
//    user's register function. Each `host.register(name, factory)`
//    pushes the factory onto a process-static `WASM_FACTORIES`
//    table and triggers an import callback. Factory ids are dense
//    integer indices into that table.
//
// 3. When the runtime later instantiates a node with a registered
//    template, JS calls `instance.exports.__reflow_pack_create_actor(id)`,
//    which looks the factory up and returns a `PackActorHandle`
//    pointer. JS hands the pointer to the runtime, which dereferences
//    it via the existing `PackActorHandle::unbox`.
//
// (1)+(2) are wired here. (3) reuses the native handle shape — the
// pack wasm and the runtime wasm share an address space when the
// runtime imports the pack's instance, so handle pointers travel
// fine. When the runtime is in a separate wasm module, the JS
// loader marshals through the pack's memory; that path is the
// browser pack-loader's responsibility, not the SDK's.

#[cfg(target_arch = "wasm32")]
mod wasm_abi {
    use super::*;
    use reflow_actor::message::Message;
    use std::collections::HashMap;
    use std::sync::Mutex;

    type Factory = Box<dyn Fn() -> std::sync::Arc<dyn Actor> + Send + Sync>;

    /// Cached per-template metadata captured at registration time so
    /// the JS adapter can declare the right inports/outports without
    /// having to instantiate the actor itself.
    struct TemplateEntry {
        factory: Factory,
        inports: Vec<String>,
        outports: Vec<String>,
    }

    /// Live actor instance held inside the pack. The runtime keeps a
    /// 1:1 mapping `(network node) ↔ (instance_id)` and routes every
    /// tick's payload through `__reflow_pack_actor_run(instance_id)`.
    struct InstanceEntry {
        actor: std::sync::Arc<dyn Actor>,
        state: std::sync::Arc<parking_lot::Mutex<dyn ActorState>>,
        load: std::sync::Arc<reflow_actor::ActorLoad>,
    }

    /// `Arc<dyn Actor>` is `!Send + !Sync` on wasm (the actor's
    /// behavior future contains JS handles), but `static` items
    /// need `Sync`. wasm32 is single-threaded so the bound is just
    /// typechecking — wrap the table in a transparent newtype with
    /// `unsafe impl Sync` to satisfy it. The runtime borrow checks
    /// inside Mutex/RefCell still police reentrancy on the one
    /// thread.
    #[repr(transparent)]
    struct WasmSync<T>(T);
    // OnceLock additionally requires Send (for cross-thread init);
    // wasm has only one thread so neither bound is observable at
    // runtime, but both have to be present for the type to compile.
    unsafe impl<T> Sync for WasmSync<T> {}
    unsafe impl<T> Send for WasmSync<T> {}
    impl<T> std::ops::Deref for WasmSync<T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.0
        }
    }

    static TEMPLATES: std::sync::OnceLock<WasmSync<Mutex<Vec<TemplateEntry>>>> =
        std::sync::OnceLock::new();
    static INSTANCES: std::sync::OnceLock<WasmSync<Mutex<HashMap<u32, InstanceEntry>>>> =
        std::sync::OnceLock::new();
    static NEXT_INSTANCE_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

    fn templates() -> &'static Mutex<Vec<TemplateEntry>> {
        &TEMPLATES.get_or_init(|| WasmSync(Mutex::new(Vec::new()))).0
    }
    fn instances() -> &'static Mutex<HashMap<u32, InstanceEntry>> {
        &INSTANCES
            .get_or_init(|| WasmSync(Mutex::new(HashMap::new())))
            .0
    }

    // The browser pack loader provides this import under the `env`
    // module — `WebAssembly.instantiate(module, { env: { … } })`.
    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        /// Provided by the JS pack loader at instantiation time.
        /// `metadata` is a UTF-8 JSON string of shape
        /// `{ "name": "...", "inports": [...], "outports": [...] }`,
        /// captured at registration so the JS-side adapter can
        /// declare the right ports without instantiating the actor.
        /// `factory_id` is what JS hands back through
        /// `__reflow_pack_create_actor`.
        fn __reflow_pack_register_template(
            metadata_ptr: *const u8,
            metadata_len: u32,
            factory_id: u32,
        );
    }

    /// Browser-side counterpart to the native [`PackHost`]. Same
    /// `host.register("name", factory)` surface; instead of writing
    /// into a C-ABI vtable, the registration call is forwarded to
    /// JS via the imported `__reflow_pack_register_template`.
    ///
    /// The phantom lifetime makes `PackHost<'a>` line up with the
    /// native struct's signature so pack source code writes
    /// `fn register(host: &mut PackHost)` either way.
    pub struct WasmPackHost<'a> {
        status: i32,
        _phantom: std::marker::PhantomData<&'a ()>,
    }

    impl<'a> WasmPackHost<'a> {
        #[doc(hidden)]
        pub fn new() -> Self {
            Self {
                status: PackRegisterStatus::Ok as i32,
                _phantom: std::marker::PhantomData,
            }
        }

        #[doc(hidden)]
        pub fn take_status(self) -> i32 {
            self.status
        }

        /// Register a template id against a factory closure. We
        /// instantiate the actor once here to read its declared
        /// inport / outport names — that lets the JS-side adapter
        /// publish the correct port shape to the runtime without
        /// any second round-trip.
        pub fn register<F>(&mut self, template_id: &str, factory: F)
        where
            F: Fn() -> std::sync::Arc<dyn Actor> + Send + Sync + 'static,
        {
            let probe = factory();
            let inports = probe.inport_names();
            let outports = probe.outport_names();
            drop(probe);

            let mut table = match templates().lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let factory_id = table.len() as u32;
            table.push(TemplateEntry {
                factory: Box::new(factory),
                inports: inports.clone(),
                outports: outports.clone(),
            });
            drop(table);

            let metadata = serde_json::json!({
                "name": template_id,
                "inports": inports,
                "outports": outports,
            })
            .to_string();
            let bytes = metadata.as_bytes();
            unsafe {
                __reflow_pack_register_template(bytes.as_ptr(), bytes.len() as u32, factory_id);
            }
        }
    }

    impl<'a> Default for WasmPackHost<'a> {
        fn default() -> Self {
            Self::new()
        }
    }

    // ─── Memory helpers ────────────────────────────────────────────

    /// Allocate `size` bytes inside the pack's wasm memory.
    /// JS uses this to write actor payloads where the pack can read
    /// them. Pair with `__reflow_pack_free` once the pack has copied
    /// out / serialized whatever it needed.
    #[unsafe(no_mangle)]
    pub extern "C" fn __reflow_pack_alloc(size: u32) -> u32 {
        if size == 0 {
            return 0;
        }
        let layout = match std::alloc::Layout::from_size_align(size as usize, 8) {
            Ok(l) => l,
            Err(_) => return 0,
        };
        // SAFETY: layout has size > 0; alloc returns null on failure.
        let ptr = unsafe { std::alloc::alloc(layout) };
        ptr as u32
    }

    /// Free a buffer previously returned by `__reflow_pack_alloc`
    /// (or by a successful `__reflow_pack_actor_run` result write).
    /// `size` MUST match the allocation size.
    #[unsafe(no_mangle)]
    pub extern "C" fn __reflow_pack_free(ptr: u32, size: u32) {
        if ptr == 0 || size == 0 {
            return;
        }
        let layout = match std::alloc::Layout::from_size_align(size as usize, 8) {
            Ok(l) => l,
            Err(_) => return,
        };
        // SAFETY: caller asserts ptr came from `__reflow_pack_alloc`
        // with this same size.
        unsafe { std::alloc::dealloc(ptr as *mut u8, layout) };
    }

    // ─── Actor lifecycle ───────────────────────────────────────────

    /// Create a fresh actor instance for `factory_id`. The pack keeps
    /// the instance alive in a static table keyed by a unique
    /// `instance_id` (returned). JS hands the id back to
    /// `__reflow_pack_actor_run` on every tick.
    ///
    /// Returns 0 on bad factory id.
    #[unsafe(no_mangle)]
    pub extern "C" fn __reflow_pack_create_actor(factory_id: u32) -> u32 {
        let table = match templates().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let entry = match table.get(factory_id as usize) {
            Some(e) => e,
            None => return 0,
        };
        let actor = (entry.factory)();
        let state = actor.create_state();
        let load = actor.load_count();
        drop(table);

        let id = NEXT_INSTANCE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut map = match instances().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        map.insert(id, InstanceEntry { actor, state, load });
        id
    }

    /// Drop the actor instance owned by `instance_id`. Any future
    /// `__reflow_pack_actor_run(instance_id, ...)` returns an error.
    #[unsafe(no_mangle)]
    pub extern "C" fn __reflow_pack_destroy_actor(instance_id: u32) {
        let mut map = match instances().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        map.remove(&instance_id);
    }

    // ─── Tick: run an actor against a JSON-encoded payload ─────────

    /// Run the actor `instance_id` once with the given JSON payload.
    ///
    /// Wire shape — JSON in, JSON out — chosen for ABI simplicity at
    /// the cost of per-tick serialization. The pack and runtime
    /// memories don't share an address space, so any cross-boundary
    /// data has to be marshalled; JSON is what the rest of the
    /// runtime already speaks.
    ///
    /// **Input** (`payload_ptr` / `payload_len`):
    /// ```json
    /// {
    ///   "input":  { "<port>": <Message>, ... },
    ///   "config": <ActorConfig>          // optional
    /// }
    /// ```
    ///
    /// **Output** — `out_ptr_ptr` and `out_len_ptr` each point at a
    /// 4-byte slot that this function writes:
    /// - on success: `*out_ptr_ptr` = ptr to a freshly-allocated
    ///   buffer holding the result JSON
    ///   (`{ "<port>": <Message>, ... }`); `*out_len_ptr` = its size.
    ///   Caller frees with `__reflow_pack_free(ptr, size)`.
    /// - on failure: same buffer holds `{ "error": "<msg>" }`;
    ///   the return value is non-zero.
    ///
    /// **Sync execution.** The actor's behavior future is driven via
    /// `pollster::block_on`. That works for actors whose futures
    /// don't await JS Promises (math, transforms, sync GPU work).
    /// Actors that `.await` `fetch` / `wgpu::map_async` will hang
    /// the call site; the next milestone integrates wasm-bindgen
    /// futures so async actors can yield to the JS event loop.
    #[unsafe(no_mangle)]
    pub extern "C" fn __reflow_pack_actor_run(
        instance_id: u32,
        payload_ptr: u32,
        payload_len: u32,
        out_ptr_ptr: u32,
        out_len_ptr: u32,
    ) -> i32 {
        // Helper: write a JSON byte buffer to the out slots.
        fn write_out(json: String, out_ptr_ptr: u32, out_len_ptr: u32) {
            let bytes = json.into_bytes();
            let len = bytes.len() as u32;
            let buf_ptr = __reflow_pack_alloc(len.max(1));
            if buf_ptr != 0 {
                // SAFETY: just allocated `len` bytes at buf_ptr.
                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf_ptr as *mut u8, bytes.len());
                }
            }
            // SAFETY: out_ptr_ptr / out_len_ptr point at JS-managed
            // memory inside the pack's wasm linear memory; JS allocated
            // them via __reflow_pack_alloc before calling.
            unsafe {
                std::ptr::write_unaligned(out_ptr_ptr as *mut u32, buf_ptr);
                std::ptr::write_unaligned(out_len_ptr as *mut u32, len);
            }
        }

        // ── Look up the actor instance.
        let instance = {
            let map = match instances().lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            match map.get(&instance_id) {
                Some(e) => (e.actor.clone(), e.state.clone(), e.load.clone()),
                None => {
                    write_out(
                        format!(r#"{{"error":"unknown instance_id {}"}}"#, instance_id),
                        out_ptr_ptr,
                        out_len_ptr,
                    );
                    return -1;
                }
            }
        };

        // ── Read + parse the payload.
        let payload_bytes = if payload_len == 0 {
            &[][..]
        } else {
            // SAFETY: JS owns this region for the duration of the
            // call; it allocated via __reflow_pack_alloc.
            unsafe { std::slice::from_raw_parts(payload_ptr as *const u8, payload_len as usize) }
        };
        let payload_str = match std::str::from_utf8(payload_bytes) {
            Ok(s) => s,
            Err(e) => {
                write_out(
                    format!(r#"{{"error":"invalid utf-8 in payload: {}"}}"#, e),
                    out_ptr_ptr,
                    out_len_ptr,
                );
                return -2;
            }
        };

        // The runtime currently only needs `input` over the wire —
        // `ActorConfig` carries channel topology and env that are
        // managed runtime-side and don't round-trip through JSON.
        // Adding serde to ActorConfig is a follow-up; for now an
        // empty config + the input payload is enough for actors
        // that don't read config-tied metadata.
        #[derive(serde::Deserialize, Default)]
        struct ActorRunPayload {
            #[serde(default)]
            input: HashMap<String, Message>,
        }

        let parsed: ActorRunPayload = match serde_json::from_str(payload_str) {
            Ok(p) => p,
            Err(e) => {
                write_out(
                    format!(r#"{{"error":"parse payload: {}"}}"#, e),
                    out_ptr_ptr,
                    out_len_ptr,
                );
                return -3;
            }
        };

        // ── Build a fresh ActorContext per tick.
        let (actor, state, load) = instance;
        let channel = flume::unbounded();
        let context = reflow_actor::ActorContext::new(
            parsed.input,
            channel,
            state,
            reflow_actor::ActorConfig::default(),
            load,
        );

        // ── Drive the future to completion. Sync-only for now.
        let future = actor.get_behavior()(context);
        let result = match pollster::block_on(future) {
            Ok(out) => out,
            Err(e) => {
                write_out(
                    format!(
                        r#"{{"error":"actor run failed: {}"}}"#,
                        e.to_string().replace('"', "\\\"")
                    ),
                    out_ptr_ptr,
                    out_len_ptr,
                );
                return -4;
            }
        };

        // ── Serialize outputs.
        let json = match serde_json::to_string(&result) {
            Ok(s) => s,
            Err(e) => {
                write_out(
                    format!(r#"{{"error":"serialize output: {}"}}"#, e),
                    out_ptr_ptr,
                    out_len_ptr,
                );
                return -5;
            }
        };
        write_out(json, out_ptr_ptr, out_len_ptr);
        0
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_abi::{
    __reflow_pack_actor_run, __reflow_pack_alloc, __reflow_pack_create_actor,
    __reflow_pack_destroy_actor, __reflow_pack_free, WasmPackHost,
};

/// Cross-target alias: pack authors always write
/// `fn register(host: &mut PackHost)`. Native binds it to the
/// vtable-borrowing struct above; wasm binds it to `WasmPackHost`,
/// which forwards registrations to a JS import instead.
#[cfg(target_arch = "wasm32")]
pub type PackHost<'a> = WasmPackHost<'a>;
