//! Host-side vtable passed into a pack's `reflow_pack_register` entrypoint.
//!
//! The vtable is deliberately narrow: one function for template
//! registration plus a `host_data` pointer that the host uses to locate the
//! per-pack registry slot. Keeping it small means future pack versions can
//! add new calls through a successor struct without breaking existing
//! packs — they'll look up the old entrypoint and the host will route
//! through a compat shim.

use std::ffi::{c_char, c_void};

use reflow_actor::Actor;
use std::sync::Arc;

/// Factory: produces a fresh actor instance per node. Mirrors
/// `Actor::create_instance`.
pub type PackFactoryFn =
    Option<unsafe extern "C" fn(user_data: *mut c_void) -> *mut PackActorHandle>;

/// Drop: called by the host when the pack is unloaded, so the pack can
/// release any heap memory it stashed in `factory_user_data`.
pub type PackFactoryDropFn = Option<unsafe extern "C" fn(user_data: *mut c_void)>;

/// Opaque actor handle the pack hands back from its factory. Layout is
/// deliberately not `#[repr(C)]` — `Arc<dyn Actor>` is a Rust fat pointer
/// whose vtable layout depends on the compiler. Lockstep-toolchain packs
/// share this layout because they link against the same `reflow_actor`
/// crate and the same rustc.
pub struct PackActorHandle {
    pub(crate) inner: Arc<dyn Actor>,
}

impl PackActorHandle {
    pub fn new(actor: Arc<dyn Actor>) -> *mut PackActorHandle {
        Box::into_raw(Box::new(PackActorHandle { inner: actor }))
    }

    /// # Safety
    ///
    /// `ptr` must be a non-null handle produced by [`Self::new`] that has
    /// not yet been unboxed. Calling this on any other pointer (including
    /// a pointer that has already been passed through `unbox`) is
    /// undefined behavior.
    pub unsafe fn unbox(ptr: *mut PackActorHandle) -> Option<Arc<dyn Actor>> {
        if ptr.is_null() {
            return None;
        }
        Some(unsafe { Box::from_raw(ptr) }.inner)
    }
}

/// Status code returned by `rfl_pack_host::register_template`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackRegisterStatus {
    Ok = 0,
    NullArg = -1,
    BadUtf8 = -2,
    Duplicate = -3,
    Internal = -99,
}

/// Vtable passed into `reflow_pack_register(host: *mut rfl_pack_host)`.
///
/// Layout is fixed — packs cast incoming pointer to this struct. Do not
/// reorder or remove fields; add new fields in a new struct version.
#[repr(C)]
pub struct PackHostVtable {
    /// Opaque pointer the host uses to locate registry state. Pack must
    /// pass this unchanged to every vtable call.
    pub host_data: *mut c_void,

    /// Register a template id with the runtime. Returns one of
    /// `PackRegisterStatus`.
    pub register_template: unsafe extern "C" fn(
        host_data: *mut c_void,
        template_id: *const c_char,
        factory: PackFactoryFn,
        drop: PackFactoryDropFn,
        factory_user_data: *mut c_void,
    ) -> i32,
}
