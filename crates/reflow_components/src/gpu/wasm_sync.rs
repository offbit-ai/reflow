//! `static`-friendly synchronization primitives for the GPU module.
//!
//! ## Background
//!
//! On native, `std::sync::Mutex<T>`, `std::sync::OnceLock<T>`, and
//! `once_cell::sync::Lazy<T>` are all `Sync` whenever `T: Send`.
//! Static items must implement `Sync`, which is why we write
//! `static FOO: Mutex<Bar> = Mutex::new(...)` without thinking.
//!
//! On `wasm32-unknown-unknown`, wgpu's WebGPU backend types
//! (`Texture`, `Buffer`, `BindGroup`, …) wrap raw JS handles
//! (`*mut u8` inside `JsValue`) and are `!Send + !Sync` — by design,
//! because a JsValue is bound to the JS thread that created it.
//! That breaks every wgpu-holding `static`. The runtime is also
//! single-threaded under wasm32 (`wasm_bindgen_futures::spawn_local`
//! everywhere) so the bound is purely typechecking — there is no
//! real cross-thread aliasing to police.
//!
//! This module provides drop-in replacements that satisfy the
//! `Sync` requirement on wasm via `unsafe impl Sync` over a
//! `RefCell<>` or an `UnsafeCell<>`, with the same surface as the
//! standard primitive on native.

#![cfg(feature = "gpu")]

// ─── GpuMutex ──────────────────────────────────────────────────────────────

/// Mutex-shaped storage for GPU caches.
///
/// Native: `std::sync::Mutex<T>`. Wasm: a `RefCell<T>` with an
/// `unsafe impl Sync` and a `lock()` that mirrors the standard
/// signature so callers compile against either backend with
/// `.lock().unwrap_or_else(|e| e.into_inner())`.
#[cfg(not(target_arch = "wasm32"))]
pub type GpuMutex<T> = std::sync::Mutex<T>;

#[cfg(target_arch = "wasm32")]
pub struct GpuMutex<T>(std::cell::RefCell<T>);

#[cfg(target_arch = "wasm32")]
// SAFETY: wasm32-unknown-unknown is single-threaded; there is no
// other thread that could race on this cell. The RefCell's runtime
// borrow check still catches reentrant aliasing on the one thread.
// `Send` is also asserted because some upstream wrappers
// (`once_cell::sync::Lazy<GpuMutex<…>>`) require it for `Sync`.
unsafe impl<T> Sync for GpuMutex<T> {}
#[cfg(target_arch = "wasm32")]
unsafe impl<T> Send for GpuMutex<T> {}

#[cfg(target_arch = "wasm32")]
impl<T> GpuMutex<T> {
    pub const fn new(value: T) -> Self {
        Self(std::cell::RefCell::new(value))
    }

    /// Mirrors `std::sync::Mutex::lock()` — `Result<Guard,
    /// PoisonError<Guard>>`. The wasm side never poisons; the
    /// `Err` branch is unreachable but the type has to exist so
    /// `.lock().unwrap_or_else(|e| e.into_inner())` typechecks.
    pub fn lock(
        &self,
    ) -> std::result::Result<
        std::cell::RefMut<'_, T>,
        std::sync::PoisonError<std::cell::RefMut<'_, T>>,
    > {
        Ok(self.0.borrow_mut())
    }
}

// ─── GpuOnceLock ───────────────────────────────────────────────────────────

/// One-shot lazy storage for GPU pipelines and other init-once
/// resources.
///
/// Native: `std::sync::OnceLock<T>`. Wasm: a `OnceCell<T>` based on
/// `UnsafeCell` with `unsafe impl Sync` — same single-thread
/// argument as `GpuMutex`. The wasm variant exposes the same
/// `.get()` and `.get_or_init(f)` shape so call sites compile
/// either way.
#[cfg(not(target_arch = "wasm32"))]
pub type GpuOnceLock<T> = std::sync::OnceLock<T>;

#[cfg(target_arch = "wasm32")]
pub struct GpuOnceLock<T> {
    cell: std::cell::UnsafeCell<Option<T>>,
}

#[cfg(target_arch = "wasm32")]
// SAFETY: wasm32 single-thread; no concurrent access possible. The
// only-once invariant is upheld by checking the slot before write.
unsafe impl<T> Sync for GpuOnceLock<T> {}
#[cfg(target_arch = "wasm32")]
unsafe impl<T> Send for GpuOnceLock<T> {}

#[cfg(target_arch = "wasm32")]
impl<T> GpuOnceLock<T> {
    pub const fn new() -> Self {
        Self {
            cell: std::cell::UnsafeCell::new(None),
        }
    }

    pub fn get(&self) -> Option<&T> {
        // SAFETY: single-threaded; the only mutation is in
        // `get_or_init`, which checks the slot before writing.
        unsafe { (*self.cell.get()).as_ref() }
    }

    pub fn get_or_init<F: FnOnce() -> T>(&self, init: F) -> &T {
        if let Some(v) = self.get() {
            return v;
        }
        // SAFETY: single-threaded; we just observed the slot was
        // empty, so writing is sound. wgpu init never reentrantly
        // calls back into the same OnceLock.
        unsafe {
            *self.cell.get() = Some(init());
            (*self.cell.get()).as_ref().expect("just inserted")
        }
    }
}
