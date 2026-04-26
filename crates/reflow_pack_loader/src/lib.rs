//! Runtime loader for Reflow actor packs.
//!
//! A pack is a cdylib that exports two symbols:
//!
//! - `reflow_pack_abi_version(): u32` — handshake against `REFLOW_PACK_ABI_VERSION`
//! - `reflow_pack_register(host: *mut PackHostVtable): i32` — calls back
//!   into the host to register template factories
//!
//! Packs can be loaded either as raw dylibs (developer loop) or wrapped in
//! a `.rflpack` zip bundle (distribution format). See [`bundle`].

pub mod bundle;
pub mod host;

use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use reflow_actor::Actor;

use bundle::{
    ABI_VERSION_SYMBOL, DEFAULT_ENTRYPOINT, PackManifest, extract_bundle, is_bundle_file,
    read_manifest,
};
use host::{PackFactoryDropFn, PackFactoryFn, PackHostVtable, PackRegisterStatus};

/// ABI version the host was built with. Packs must emit the same value
/// from their `reflow_pack_abi_version` symbol.
pub const REFLOW_PACK_ABI_VERSION: u32 = {
    // Fallback for rust-analyzer / IDEs where build.rs hasn't run yet.
    let s = match option_env!("REFLOW_PACK_ABI_VERSION") {
        Some(s) => s,
        None => "0",
    };
    parse_u32(s.as_bytes())
};

/// Host triple the loader was compiled for. Used to pick the right dylib
/// out of a `.rflpack` manifest.
pub const REFLOW_PACK_HOST_TRIPLE: &str = match option_env!("REFLOW_PACK_HOST_TRIPLE") {
    Some(s) => s,
    None => "unknown",
};

const fn parse_u32(bytes: &[u8]) -> u32 {
    let mut out: u32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < b'0' || b > b'9' {
            return 0;
        }
        out = out * 10 + (b - b'0') as u32;
        i += 1;
    }
    out
}

// ─── registry ──────────────────────────────────────────────────────────────

struct FactoryRecord {
    factory: PackFactoryFn,
    drop: PackFactoryDropFn,
    user_data: SendPtr,
    // Keep the owning library pinned for the lifetime of the factory.
    _owner: Arc<LoadedPack>,
}

impl Drop for FactoryRecord {
    fn drop(&mut self) {
        if let Some(drop_fn) = self.drop {
            unsafe { drop_fn(self.user_data.0) };
        }
    }
}

struct SendPtr(*mut c_void);
// Pack factories are explicitly documented as needing to be callable from
// any thread. The pack author is responsible for making user_data Send+Sync.
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

struct LoadedPack {
    name: String,
    version: String,
    #[allow(dead_code)]
    manifest: Option<PackManifest>,
    source_path: PathBuf,
    templates: parking_lot::Mutex<Vec<String>>,
    /// The dlopen handle. Native-only — wasm packs are loaded via
    /// `WebAssembly.instantiate` from JS and registered through a
    /// separate path that doesn't keep a Rust-side handle.
    #[cfg(not(target_arch = "wasm32"))]
    _lib: libloading::Library,
}

pub struct PackRegistry {
    inner: RwLock<PackRegistryInner>,
}

struct PackRegistryInner {
    /// Template-id → factory record. First-registered wins; duplicates
    /// are rejected.
    templates: HashMap<String, Arc<FactoryRecord>>,
    /// Keyed by manifest.name (or file stem for raw dylib loads).
    loaded: HashMap<String, Arc<LoadedPack>>,
}

impl PackRegistry {
    fn new() -> Self {
        Self {
            inner: RwLock::new(PackRegistryInner {
                templates: HashMap::new(),
                loaded: HashMap::new(),
            }),
        }
    }

    /// Return a fresh actor instance for `template_id` if a pack has
    /// registered it. `None` means "not a pack-owned template — fall back
    /// to the bundled catalog."
    pub fn instantiate(&self, template_id: &str) -> Option<Arc<dyn Actor>> {
        let record = self.inner.read().templates.get(template_id).cloned()?;
        let factory = record.factory?;
        let ptr = unsafe { factory(record.user_data.0) };
        if ptr.is_null() {
            return None;
        }
        unsafe { host::PackActorHandle::unbox(ptr) }
    }

    /// List every template id any loaded pack has published.
    pub fn template_ids(&self) -> Vec<String> {
        let g = self.inner.read();
        let mut v: Vec<String> = g.templates.keys().cloned().collect();
        v.sort();
        v
    }

    /// List every loaded pack's name + version + owning template count.
    pub fn loaded_packs(&self) -> Vec<LoadedPackInfo> {
        let g = self.inner.read();
        g.loaded
            .values()
            .map(|p| LoadedPackInfo {
                name: p.name.clone(),
                version: p.version.clone(),
                source_path: p.source_path.clone(),
                templates: p.templates.lock().clone(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LoadedPackInfo {
    pub name: String,
    pub version: String,
    pub source_path: PathBuf,
    pub templates: Vec<String>,
}

/// Process-global pack registry. Packs are shared across every runtime
/// handle in the process — matching the behaviour of the bundled component
/// catalog (also a global static).
pub static PACK_REGISTRY: Lazy<PackRegistry> = Lazy::new(PackRegistry::new);

// ─── load ──────────────────────────────────────────────────────────────────

/// Load a pack from either a raw dylib path or a `.rflpack` bundle.
///
/// Idempotent per pack name: loading the same pack twice is a no-op (returns
/// `Ok` with the previously-registered template set reported via
/// [`PackRegistry::loaded_packs`]).
///
/// **Native only.** wasm32 targets load packs through
/// `WebAssembly.instantiate` from the JS side; this `dlopen`-based
/// path is excluded there because `libloading` itself doesn't compile.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_pack<P: AsRef<Path>>(path: P) -> Result<Vec<String>> {
    let path = path.as_ref();
    let canonical =
        std::fs::canonicalize(path).with_context(|| format!("canonicalize {}", path.display()))?;

    let (dylib_path, manifest): (PathBuf, Option<PackManifest>) =
        if is_bundle_file(&canonical).unwrap_or(false) {
            let extracted =
                extract_bundle(&canonical, REFLOW_PACK_ABI_VERSION, REFLOW_PACK_HOST_TRIPLE)?;
            (extracted.dylib_path, Some(extracted.manifest))
        } else {
            (canonical.clone(), None)
        };

    let pack_name = manifest
        .as_ref()
        .map(|m| m.name.clone())
        .unwrap_or_else(|| {
            canonical
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("pack")
                .to_string()
        });
    let pack_version = manifest
        .as_ref()
        .map(|m| m.version.clone())
        .unwrap_or_else(|| "0.0.0".to_string());
    let entrypoint_name = manifest
        .as_ref()
        .map(|m| m.entrypoint.clone())
        .unwrap_or_else(|| DEFAULT_ENTRYPOINT.to_string());

    // Already loaded? Report the existing template set without reloading.
    {
        let g = PACK_REGISTRY.inner.read();
        if let Some(existing) = g.loaded.get(&pack_name) {
            return Ok(existing.templates.lock().clone());
        }
    }

    // dlopen the dylib. The library's lifetime is tied to the LoadedPack
    // Arc — once dropped, the OS unloads the image.
    let lib = unsafe {
        libloading::Library::new(&dylib_path)
            .with_context(|| format!("dlopen {}", dylib_path.display()))?
    };

    // ABI symbol first — cheap sanity check even if manifest already
    // vouched for it.
    unsafe {
        let sym: libloading::Symbol<unsafe extern "C" fn() -> u32> = lib
            .get(ABI_VERSION_SYMBOL.as_bytes())
            .with_context(|| format!("pack missing symbol `{ABI_VERSION_SYMBOL}`"))?;
        let v = sym();
        if v != REFLOW_PACK_ABI_VERSION {
            bail!(
                "pack `{pack_name}` ABI {v} != host ABI {} — rebuild pack against current toolchain",
                REFLOW_PACK_ABI_VERSION
            );
        }
    }

    let owner = Arc::new(LoadedPack {
        name: pack_name.clone(),
        version: pack_version,
        manifest,
        source_path: canonical,
        templates: parking_lot::Mutex::new(Vec::new()),
        _lib: lib,
    });

    // Invoke the pack's register function. It calls back into us via
    // `register_template_trampoline` with a per-load key so we can attribute
    // factories to this pack.
    {
        let lib = &owner._lib;
        let register: libloading::Symbol<unsafe extern "C" fn(*mut PackHostVtable) -> i32> = unsafe {
            lib.get(entrypoint_name.as_bytes())
                .with_context(|| format!("pack missing symbol `{entrypoint_name}`"))?
        };
        let key = RegisterKey::new(Arc::clone(&owner));
        let key_box = Box::into_raw(Box::new(key));
        let mut vtable = PackHostVtable {
            host_data: key_box as *mut c_void,
            register_template: register_template_trampoline,
        };
        let status = unsafe { register(&mut vtable as *mut PackHostVtable) };
        // Reclaim the key so we don't leak it.
        let _ = unsafe { Box::from_raw(key_box) };
        if status != PackRegisterStatus::Ok as i32 {
            bail!("pack `{pack_name}` register returned status {status} — registration aborted");
        }
    }

    // Publish.
    let templates = owner.templates.lock().clone();
    PACK_REGISTRY
        .inner
        .write()
        .loaded
        .insert(pack_name, Arc::clone(&owner));
    Ok(templates)
}

/// Read the manifest from a `.rflpack` without loading any code. Fails on
/// raw dylibs (there's no manifest to read).
pub fn inspect_pack<P: AsRef<Path>>(path: P) -> Result<PackManifest> {
    let path = path.as_ref();
    if !is_bundle_file(path).unwrap_or(false) {
        bail!("inspect_pack requires a .rflpack bundle — raw dylibs have no manifest");
    }
    read_manifest(path)
}

// ─── host vtable implementation ────────────────────────────────────────────

/// Per-load key handed to the pack through `host_data`. The trampoline
/// downcasts the pointer on every call.
struct RegisterKey {
    owner: Arc<LoadedPack>,
}

impl RegisterKey {
    fn new(owner: Arc<LoadedPack>) -> Self {
        Self { owner }
    }
}

unsafe extern "C" fn register_template_trampoline(
    host_data: *mut c_void,
    template_id: *const c_char,
    factory: PackFactoryFn,
    drop: PackFactoryDropFn,
    factory_user_data: *mut c_void,
) -> i32 {
    if host_data.is_null() || template_id.is_null() || factory.is_none() {
        return PackRegisterStatus::NullArg as i32;
    }
    let key = unsafe { &*(host_data as *const RegisterKey) };
    let id = match unsafe { CStr::from_ptr(template_id) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return PackRegisterStatus::BadUtf8 as i32,
    };

    let record = Arc::new(FactoryRecord {
        factory,
        drop,
        user_data: SendPtr(factory_user_data),
        _owner: Arc::clone(&key.owner),
    });

    let mut g = PACK_REGISTRY.inner.write();
    if g.templates.contains_key(&id) {
        return PackRegisterStatus::Duplicate as i32;
    }
    g.templates.insert(id.clone(), record);
    key.owner.templates.lock().push(id);
    PackRegisterStatus::Ok as i32
}

// ─── utilities ─────────────────────────────────────────────────────────────

/// True if the given template id is owned by a loaded pack. Cheap —
/// callers can use this to decide whether to consult the pack registry or
/// fall straight through to the bundled catalog.
pub fn has_template(template_id: &str) -> bool {
    PACK_REGISTRY
        .inner
        .read()
        .templates
        .contains_key(template_id)
}

/// Convenience wrapper around [`PackRegistry::instantiate`] on the global
/// registry.
pub fn instantiate(template_id: &str) -> Option<Arc<dyn Actor>> {
    PACK_REGISTRY.instantiate(template_id)
}

/// Convenience: JSON array of pack info, matching the wire format
/// `rfl_pack_list_json` surfaces.
pub fn list_packs_json() -> Result<String> {
    let list = PACK_REGISTRY.loaded_packs();
    serde_json::to_string(&list).map_err(|e| anyhow!("serialize pack list: {e}"))
}

// Silence unused-import warnings when the crate's only callers are across
// the cdylib boundary.
#[allow(dead_code)]
fn _types_used() {
    let _: PackFactoryFn = None;
    let _: PackFactoryDropFn = None;
}
