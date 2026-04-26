//! Process-global GPU context — shared wgpu Device + Queue.
//!
//! ## Why two storage paths?
//!
//! On native targets, wgpu's Device/Queue are `Send + Sync`, so we
//! can stash an initialized `GpuContext` behind a sync `Lazy<>` and
//! drive the (async) wgpu init synchronously via `pollster::block_on`.
//! All accesses go through `&*GPU_CONTEXT` and resolve transparently.
//!
//! On `wasm32-unknown-unknown` neither premise holds:
//!
//! 1. wgpu's WebGPU backend types (`Device`, `Queue`, `Texture`, …)
//!    wrap raw JS handles (`*mut u8` inside `JsValue`) and are
//!    `!Send + !Sync` — by construction. The Rust runtime is also
//!    single-threaded under wasm32 so multi-thread aliasing is
//!    impossible; the bound is just typechecking, not real safety.
//! 2. `pollster::block_on` panics in browsers (no blocking allowed
//!    on the JS event loop).
//!
//! The wasm path therefore:
//!
//! - Stores the context in a `OnceLock<WasmSyncCell<GpuContext>>`.
//!   `WasmSyncCell` is a thin newtype with an `unsafe impl Send + Sync`
//!   that's sound under the single-threaded wasm runtime.
//! - Requires explicit async init via [`init_gpu_context`] before any
//!   GPU actor runs. Callers do this once during workflow startup,
//!   typically in their JS bootstrap:
//!     ```js
//!     import { ready, initGpuContext, Network } from "@offbit-ai/reflow";
//!     await ready();
//!     await initGpuContext();
//!     // ...build network and start
//!     ```
//!
//! ## API uniformity
//!
//! Call sites use `try_gpu_context()` on both targets — it returns
//! `Result<&'static GpuContext, String>` either way. Code that
//! previously dereferenced `&*GPU_CONTEXT` on native should migrate
//! to `try_gpu_context()?` so the same code path compiles for wasm.
//! `GPU_CONTEXT` itself remains a native-only export for the
//! existing call sites that haven't migrated yet.

use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use once_cell::sync::Lazy;

#[cfg(target_arch = "wasm32")]
use std::sync::OnceLock;

/// On wasm, wgpu types are `!Send + !Sync` (they hold raw JS handles).
/// The runtime is single-threaded so the bounds are just typechecking
/// boilerplate; this newtype unsafely asserts them so we can put the
/// context behind a `OnceLock<>`.
#[cfg(target_arch = "wasm32")]
#[repr(transparent)]
struct WasmSyncCell<T>(T);

#[cfg(target_arch = "wasm32")]
// SAFETY: wasm32-unknown-unknown is single-threaded — there is no
// other thread that could observe these handles. The wasm-bindgen
// docs are explicit that JsValue handles are tied to the JS thread
// they were created on, which on wasm32 is the only thread.
unsafe impl<T> Send for WasmSyncCell<T> {}
#[cfg(target_arch = "wasm32")]
unsafe impl<T> Sync for WasmSyncCell<T> {}

#[cfg(target_arch = "wasm32")]
impl<T> std::ops::Deref for WasmSyncCell<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

// ─── Storage ───────────────────────────────────────────────────────────────

/// Process-global GPU context (native — sync init via pollster).
#[cfg(not(target_arch = "wasm32"))]
pub static GPU_CONTEXT: Lazy<GpuContext> = Lazy::new(|| {
    pollster::block_on(GpuContext::init(None)).expect("Failed to initialize GPU context")
});

#[cfg(not(target_arch = "wasm32"))]
static GPU_CONTEXT_RESULT: Lazy<Result<GpuContext, String>> =
    Lazy::new(|| pollster::block_on(GpuContext::init(None)));

/// Process-global GPU context (wasm — populated by [`init_gpu_context`]).
#[cfg(target_arch = "wasm32")]
static GPU_CONTEXT_CELL: OnceLock<WasmSyncCell<GpuContext>> = OnceLock::new();

// ─── GpuContext ────────────────────────────────────────────────────────────

pub struct GpuContext {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    /// Kept on wasm so we can call `Surface::get_default_config`
    /// (which takes `&Adapter`) when the canvas is resized.
    /// Off-screen-only native builds don't need this.
    #[cfg(target_arch = "wasm32")]
    adapter: Arc<wgpu::Adapter>,
    /// On wasm, a target Surface created from the user-provided
    /// canvas. Off-screen renderers (SDF, scene_render readback)
    /// don't need it; on-canvas rendering paths read from here.
    /// `None` on native or if init was called without a selector.
    #[cfg(target_arch = "wasm32")]
    surface: Option<wgpu::Surface<'static>>,
}

impl GpuContext {
    /// Initialize the GPU context.
    ///
    /// `canvas_selector` is wasm-only: a CSS selector pointing to the
    /// `HTMLCanvasElement` Reflow should render to. When provided, a
    /// `wgpu::Surface` is created from the canvas and reused as the
    /// `compatible_surface` for adapter selection — this matters
    /// because Chromium's WebGPU implementation will fail to create
    /// a render-target-capable adapter without one. Pass `None` for
    /// off-screen-only workloads (SDF, mesh ops) where readback to
    /// CPU is the final step.
    async fn init(
        #[allow(unused_variables)] canvas_selector: Option<&str>,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // wasm: build the canvas surface up-front so request_adapter
        // can pick a presentation-capable adapter.
        #[cfg(target_arch = "wasm32")]
        let surface = if let Some(selector) = canvas_selector {
            use wasm_bindgen::JsCast;
            let window = web_sys::window().ok_or("no `window` in this context")?;
            let document = window.document().ok_or("no `document` in this context")?;
            let canvas = document
                .query_selector(selector)
                .map_err(|e| format!("query_selector failed: {:?}", e))?
                .ok_or_else(|| format!("no element matched canvas selector `{}`", selector))?
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .map_err(|_| format!("element matching `{}` is not an HTMLCanvasElement", selector))?;
            let target = wgpu::SurfaceTarget::Canvas(canvas);
            Some(
                instance
                    .create_surface(target)
                    .map_err(|e| format!("create_surface from canvas failed: {}", e))?,
            )
        } else {
            None
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                #[cfg(target_arch = "wasm32")]
                compatible_surface: surface.as_ref(),
                #[cfg(not(target_arch = "wasm32"))]
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or("No GPU adapter found")?;
        let info = adapter.get_info();
        // Native uses eprintln; on wasm we route through the console
        // so messages surface in DevTools instead of being lost.
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!(
            "[gpu] backend={:?} device='{}' vendor={} device_type={:?}",
            info.backend, info.name, info.vendor, info.device_type
        );
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(
            &format!(
                "[gpu] backend={:?} device='{}' vendor={} device_type={:?}",
                info.backend, info.name, info.vendor, info.device_type
            )
            .into(),
        );

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Reflow Shared GPU"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|e| format!("GPU device request failed: {}", e))?;

        // The surface (if any) is left unconfigured here — its
        // width/height aren't known until the rendering actor knows
        // the canvas size. Callers invoke `configure_surface(w, h)`
        // before issuing the first frame.
        Ok(Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            #[cfg(target_arch = "wasm32")]
            adapter: Arc::new(adapter),
            #[cfg(target_arch = "wasm32")]
            surface,
        })
    }

    /// Browser-side accessor for the canvas surface. Returns `None`
    /// when init was called without a canvas selector (off-screen
    /// workloads) or on native targets.
    #[cfg(target_arch = "wasm32")]
    pub fn surface(&self) -> Option<&wgpu::Surface<'static>> {
        self.surface.as_ref()
    }

    /// Configure the canvas surface for a given pixel size. Picks
    /// an sRGB format if the adapter supports one, falls back to
    /// the adapter's first preference otherwise. Idempotent — safe
    /// to call again on canvas resize.
    #[cfg(target_arch = "wasm32")]
    pub fn configure_surface(&self, width: u32, height: u32) -> Result<(), String> {
        let surface = self
            .surface
            .as_ref()
            .ok_or("GPU context was initialized without a canvas")?;
        let mut config = surface
            .get_default_config(&self.adapter, width.max(1), height.max(1))
            .ok_or("surface incompatible with adapter")?;
        // Prefer sRGB if available so colors round-trip correctly
        // through the canvas pipeline.
        let caps = surface.get_capabilities(&self.adapter);
        if let Some(srgb) = caps.formats.iter().copied().find(|f| f.is_srgb()) {
            config.format = srgb;
        }
        surface.configure(&self.device, &config);
        Ok(())
    }

    /// Shared device reference.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Shared queue reference.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Submit command buffers. Do NOT poll here — the caller should call
    /// device.poll() after map_async to flush both the render and the
    /// readback in a single poll, avoiding double-poll deadlocks on Metal.
    pub fn submit_and_poll(&self, command_buffer: wgpu::CommandBuffer) {
        self.queue.submit(std::iter::once(command_buffer));
    }
}

// ─── Public accessors ──────────────────────────────────────────────────────

/// Async, idempotent GPU context initializer.
///
/// On wasm32, callers must run this once before any GPU actor —
/// pass a CSS selector for the `<canvas>` element to render into.
/// `None` is valid for off-screen-only workloads (mesh ops, SDF
/// readback) where the result is read back to CPU and displayed
/// outside the wgpu pipeline.
///
/// On native this is a thin force-init wrapper around the lazy
/// static — the `canvas_selector` argument is ignored. Surface
/// creation on native uses winit / a window, not a CSS selector.
#[cfg(target_arch = "wasm32")]
pub async fn init_gpu_context(canvas_selector: Option<&str>) -> Result<(), String> {
    if GPU_CONTEXT_CELL.get().is_some() {
        return Ok(());
    }
    let ctx = GpuContext::init(canvas_selector).await?;
    // `set` returns Err if another caller raced us; that's fine —
    // the other side won and we drop our context.
    let _ = GPU_CONTEXT_CELL.set(WasmSyncCell(ctx));
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn init_gpu_context(_canvas_selector: Option<&str>) -> Result<(), String> {
    Lazy::force(&GPU_CONTEXT);
    Ok(())
}

/// Fallible accessor — preferred at every call site.
///
/// On native this is the result of the lazy init (which may have
/// failed in headless environments). On wasm this returns the
/// context populated by [`init_gpu_context`], or an error if init
/// hasn't been run yet.
pub fn try_gpu_context() -> Result<&'static GpuContext, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        match &*GPU_CONTEXT_RESULT {
            Ok(ctx) => Ok(ctx),
            Err(err) => Err(err.clone()),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        GPU_CONTEXT_CELL
            .get()
            .map(|cell| &**cell)
            .ok_or_else(|| {
                "GPU context not initialized — call `initGpuContext()` (or \
                 reflow_components::gpu::init_gpu_context().await) before \
                 constructing GPU actors"
                    .to_string()
            })
    }
}
