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
    use std::sync::Mutex;

    type Factory = Box<dyn Fn() -> std::sync::Arc<dyn Actor> + Send + Sync>;

    // Single-thread on wasm; Mutex picked for `Send + Sync` rather
    // than for actual locking. `std::sync::OnceLock` is in
    // core/std and avoids pulling once_cell into a SDK that pack
    // authors depend on.
    static WASM_FACTORIES: std::sync::OnceLock<Mutex<Vec<Factory>>> =
        std::sync::OnceLock::new();
    fn factories() -> &'static Mutex<Vec<Factory>> {
        WASM_FACTORIES.get_or_init(|| Mutex::new(Vec::new()))
    }

    // The browser pack loader provides this import under the `env`
    // module — `WebAssembly.instantiate(module, { env: { … } })`.
    // Without `wasm_import_module` the linker emits an unresolved
    // symbol error instead of a wasm import entry.
    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        /// Provided by the JS pack loader at instantiation time.
        /// Receives a UTF-8 name pointer + length plus the factory
        /// id assigned by `WasmPackHost`. JS uses the id later to
        /// route actor-creation requests back through
        /// `__reflow_pack_create_actor`.
        fn __reflow_pack_register_template(
            name_ptr: *const u8,
            name_len: u32,
            factory_id: u32,
        );
    }

    /// Browser-side counterpart to the native [`PackHost`]. Same
    /// surface — the user calls `host.register("name", factory)`
    /// from inside their `#[reflow_pack]` function — but instead of
    /// writing into a C-ABI vtable, the registration call is
    /// forwarded to JS via the imported
    /// `__reflow_pack_register_template` function.
    ///
    /// The lifetime parameter is a `PhantomData` here so the wasm
    /// `PackHost<'a>` alias matches the native struct's signature
    /// — pack source code writes `fn register(host: &mut PackHost)`
    /// either way.
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

        /// Register a template id against a factory closure. Same
        /// shape as the native `PackHost::register`.
        pub fn register<F>(&mut self, template_id: &str, factory: F)
        where
            F: Fn() -> std::sync::Arc<dyn Actor> + Send + Sync + 'static,
        {
            let mut table = match factories().lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let factory_id = table.len() as u32;
            table.push(Box::new(factory));
            drop(table);

            let bytes = template_id.as_bytes();
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

    /// Browser-side actor-creation entrypoint. JS calls this with
    /// the factory id it stashed during `__reflow_pack_register` and
    /// receives a `*mut PackActorHandle` it can route through the
    /// runtime's existing `PackActorHandle::unbox` path.
    ///
    /// Returns null if the id is out of bounds.
    #[unsafe(no_mangle)]
    pub extern "C" fn __reflow_pack_create_actor(factory_id: u32) -> *mut PackActorHandle {
        let table = match factories().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match table.get(factory_id as usize) {
            Some(factory) => {
                let actor = factory();
                PackActorHandle::new(actor)
            }
            None => std::ptr::null_mut(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_abi::{WasmPackHost, __reflow_pack_create_actor};

/// Cross-target alias: pack authors always write
/// `fn register(host: &mut PackHost)`. Native binds it to the
/// vtable-borrowing struct above; wasm binds it to `WasmPackHost`,
/// which forwards registrations to a JS import instead.
#[cfg(target_arch = "wasm32")]
pub type PackHost<'a> = WasmPackHost<'a>;
