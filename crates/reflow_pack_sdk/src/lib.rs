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
pub struct PackHost<'a> {
    vtable: &'a mut PackHostVtable,
    status: i32,
}

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

unsafe extern "C" fn factory_trampoline(user_data: *mut c_void) -> *mut PackActorHandle {
    if user_data.is_null() {
        return std::ptr::null_mut();
    }
    let factory = unsafe { &*(user_data as *const Box<dyn Fn() -> Arc<dyn Actor> + Send + Sync>) };
    let actor = factory();
    PackActorHandle::new(actor)
}

unsafe extern "C" fn factory_drop_trampoline(user_data: *mut c_void) {
    if user_data.is_null() {
        return;
    }
    unsafe {
        let _ = Box::from_raw(user_data as *mut Box<dyn Fn() -> Arc<dyn Actor> + Send + Sync>);
    }
}

pub use reflow_actor::{ActorBehavior, ActorLoad, ActorPayload, ActorState, MemoryState, Port};
