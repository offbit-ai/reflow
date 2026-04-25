//! `#[reflow_pack]` — wraps a user function `fn(&mut PackHost)` with the
//! two `extern "C"` entrypoints `reflow_pack_loader` expects a pack cdylib
//! to export.

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
///
/// The macro emits:
/// - `extern "C" fn reflow_pack_abi_version() -> u32`
/// - `extern "C" fn reflow_pack_register(host: *mut PackHostVtable) -> i32`
///
/// which wrap the user function. The user function's body receives a
/// safe `&mut PackHost`; panics inside it are caught and reported as a
/// non-zero status to the loader.
#[proc_macro_attribute]
pub fn reflow_pack(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let user_ident = &func.sig.ident;

    let expanded = quote! {
        #func

        #[no_mangle]
        pub extern "C" fn reflow_pack_abi_version() -> u32 {
            ::reflow_pack_sdk::REFLOW_PACK_ABI_VERSION
        }

        #[no_mangle]
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
    };

    TokenStream::from(expanded)
}
