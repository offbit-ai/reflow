//! C ABI surface for the pack loader.
//!
//! Every call here is a thin shim over `reflow_pack_loader`. Bundled
//! template resolution checks the pack registry first (see
//! `catalog::rfl_template_actor_new`) so the same `rfl_template_actor_new`
//! call used for built-in actors also returns pack-supplied actors once
//! the pack is loaded.

#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use crate::{rfl_status, set_last_error};

/// Load an actor pack from `path`. `path` can point at either:
/// - a `.rflpack` bundle (detected by zip magic), or
/// - a raw cdylib (`.so`, `.dylib`, `.dll`) — useful during pack
///   development.
///
/// Returns `rfl_status::Ok` and, on success, writes a JSON array of newly
/// registered template ids into `*out_templates_json`. Caller frees the
/// JSON via `rfl_string_free`.
///
/// Loading the same pack twice (by manifest name, or by file stem for
/// raw dylibs) is a no-op — the existing template set is returned
/// unchanged. This makes the call safe to invoke from repeated workflow
/// boots.
#[no_mangle]
pub unsafe extern "C" fn rfl_pack_load(
    path: *const c_char,
    out_templates_json: *mut *mut c_char,
) -> rfl_status {
    crate::clear_last_error();
    if path.is_null() {
        set_last_error("path is null");
        return rfl_status::NullArg;
    }
    let p = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("path is not valid UTF-8");
            return rfl_status::InvalidUtf8;
        }
    };

    match reflow_pack_loader::load_pack(p) {
        Ok(templates) => {
            if !out_templates_json.is_null() {
                let json = serde_json::to_string(&templates).unwrap_or_else(|_| "[]".into());
                let c = CString::new(json).unwrap_or_default();
                unsafe {
                    *out_templates_json = c.into_raw();
                }
            }
            rfl_status::Ok
        }
        Err(e) => {
            set_last_error(format!("pack load '{p}': {e:#}"));
            rfl_status::Runtime
        }
    }
}

/// Return a JSON array describing every loaded pack:
/// `[{ "name": ..., "version": ..., "source_path": ..., "templates": [...] }]`.
/// Caller frees via `rfl_string_free`.
#[no_mangle]
pub extern "C" fn rfl_pack_list_json() -> *mut c_char {
    crate::clear_last_error();
    let list = reflow_pack_loader::PACK_REGISTRY.loaded_packs();
    match serde_json::to_string(&list) {
        Ok(s) => CString::new(s)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(e) => {
            set_last_error(format!("pack list serialize: {e}"));
            std::ptr::null_mut()
        }
    }
}

/// Read a `.rflpack` manifest without loading the pack. Fails for raw
/// dylibs (they have no manifest). Caller frees the returned JSON via
/// `rfl_string_free`.
#[no_mangle]
pub unsafe extern "C" fn rfl_pack_inspect_json(path: *const c_char) -> *mut c_char {
    crate::clear_last_error();
    if path.is_null() {
        set_last_error("path is null");
        return std::ptr::null_mut();
    }
    let p = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("path is not valid UTF-8");
            return std::ptr::null_mut();
        }
    };
    match reflow_pack_loader::inspect_pack(p) {
        Ok(m) => match serde_json::to_string(&m) {
            Ok(s) => CString::new(s)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            Err(e) => {
                set_last_error(format!("manifest serialize: {e}"));
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            set_last_error(format!("pack inspect '{p}': {e:#}"));
            std::ptr::null_mut()
        }
    }
}

/// The pack ABI version this host was compiled with. Pack authors can
/// stamp the same number into their `.rflpack` manifests via
/// `reflow-pack build` (which reads the `REFLOW_PACK_ABI_VERSION` env var
/// or `[abi]` section).
#[no_mangle]
pub extern "C" fn rfl_pack_abi_version() -> u32 {
    reflow_pack_loader::REFLOW_PACK_ABI_VERSION
}
