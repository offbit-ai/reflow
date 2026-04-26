//! `#[reflow_pack]` — wraps a user function `fn(&mut PackHost)` with
//! the entrypoints the loader expects a pack to export.
//!
//! ## Two ABIs, one source crate
//!
//! Pack source crates produce two distinct binary shapes from the
//! same Rust:
//!
//! - **Native cdylib** (`dlopen` / `LoadLibrary`). Loader passes a
//!   `*mut PackHostVtable` through the standard C ABI; the pack's
//!   register function fills it with template factories and returns
//!   a status.
//! - **Browser wasm** (`WebAssembly.instantiate`). The pack wasm
//!   imports a single JS callback (`__reflow_pack_register_template`),
//!   and exports a no-arg registration function the JS loader calls
//!   right after instantiation. The pack walks the user's register
//!   function once, calling the JS import for each template, then
//!   stashes the factories in a process-static table keyed by an
//!   integer id. JS hands those ids back to
//!   `__reflow_pack_create_actor(id)` whenever the runtime needs a
//!   fresh actor instance.
//!
//! Both ABIs route through the *same* user-written register function;
//! the macro emits whichever entrypoints are appropriate for the
//! target. See `crates/reflow_pack_loader/src/lib.rs` for the native
//! loader and `sdk/node/reflow.browser.mjs::loadPack` for the
//! browser one.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

/// Mark a function as the pack's registration entrypoint.
///
/// ```ignore
/// use reflow_pack_sdk::{reflow_pack, PackHost};
///
/// #[reflow_pack]
/// fn register(host: &mut PackHost) {
///     host.register("my.pack.hello", || std::sync::Arc::new(HelloActor));
/// }
/// ```
#[proc_macro_attribute]
pub fn reflow_pack(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let user_ident = &func.sig.ident;

    let expanded = quote! {
        #func

        #[unsafe(no_mangle)]
        pub extern "C" fn reflow_pack_abi_version() -> u32 {
            ::reflow_pack_sdk::REFLOW_PACK_ABI_VERSION
        }

        // ─── Native ABI ──────────────────────────────────────────────
        // dlopen-style: loader passes a *mut PackHostVtable, our
        // register function fills it with template factories.
        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn reflow_pack_register(
            host: *mut ::reflow_pack_sdk::PackHostVtable,
        ) -> i32 {
            if host.is_null() {
                return ::reflow_pack_sdk::PackRegisterStatus::NullArg as i32;
            }
            let result = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                let mut wrapper = ::reflow_pack_sdk::PackHost::from_vtable(unsafe { &mut *host });
                #user_ident(&mut wrapper);
                wrapper.take_status()
            }));
            match result {
                Ok(status) => status,
                Err(_) => ::reflow_pack_sdk::PackRegisterStatus::Internal as i32,
            }
        }

        // ─── Wasm ABI ────────────────────────────────────────────────
        // The browser-side pack loader instantiates the pack wasm
        // with a single import — `__reflow_pack_register_template`
        // — then calls our `__reflow_pack_register` export. We walk
        // the user's register function once, invoke the import per
        // template (handing it a name-bytes pointer + length + a
        // factory id), and the JS side registers each template with
        // the host runtime.
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn __reflow_pack_register() -> i32 {
            let result = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                let mut wrapper = ::reflow_pack_sdk::WasmPackHost::new();
                #user_ident(&mut wrapper);
                wrapper.take_status()
            }));
            match result {
                Ok(status) => status,
                Err(_) => ::reflow_pack_sdk::PackRegisterStatus::Internal as i32,
            }
        }
    };

    TokenStream::from(expanded)
}
