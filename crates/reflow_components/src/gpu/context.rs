//! Process-global GPU context — shared wgpu Device + Queue.
//!
//! Initialized lazily on first access. All GPU actors share the same
//! device and queue, eliminating the ~500ms per-call init overhead.
//!
//! Pattern follows `STREAM_REGISTRY` in `reflow_actor::stream`.

use once_cell::sync::Lazy;
use std::sync::Arc;

/// Process-global GPU context. Initialized lazily on first access.
pub static GPU_CONTEXT: Lazy<GpuContext> =
    Lazy::new(|| pollster::block_on(GpuContext::init()).expect("Failed to initialize GPU context"));

/// Fallible GPU context access for render paths that should degrade to an actor
/// error instead of poisoning a global singleton in headless environments.
static GPU_CONTEXT_RESULT: Lazy<Result<GpuContext, String>> =
    Lazy::new(|| pollster::block_on(GpuContext::init()));

pub struct GpuContext {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
}

impl GpuContext {
    async fn init() -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or("No GPU adapter found")?;
        let info = adapter.get_info();
        eprintln!(
            "[gpu] backend={:?} device='{}' vendor={} device_type={:?}",
            info.backend, info.name, info.vendor, info.device_type
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
        Ok(Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
        })
    }

    /// Shared device reference. Thread-safe (wgpu::Device is Send+Sync).
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Shared queue reference. Thread-safe (wgpu::Queue is Send+Sync).
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

pub fn try_gpu_context() -> Result<&'static GpuContext, String> {
    match &*GPU_CONTEXT_RESULT {
        Ok(ctx) => Ok(ctx),
        Err(err) => Err(err.clone()),
    }
}
