//! SDF Live Render Actor — persistent GPU pipeline for real-time ray marching.
//!
//! Unlike SdfRenderActor which recompiles everything per invocation,
//! this actor caches the pipeline and only updates uniforms per frame.
//! Outputs raw RGBA bytes — downstream actors decide the render target.
//!
//! Caching strategy:
//! - SDF IR change → recompile WGSL, hash, cache pipeline (rare, ~50ms)
//! - Camera/time change → write uniform buffer only (per-frame, <1ms)
//! - Resolution change → recreate texture + readback buffers (rare)

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{
    message::EncodableValue,
    stream::{StreamFrame, STREAM_REGISTRY},
    ActorContext,
};
use reflow_sdf::ir::{SceneSettings, SdfNode};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use once_cell::sync::Lazy;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Cached SDF compute pipeline — keyed by WGSL hash.
struct CachedSdfPipeline {
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
}

/// Process-global SDF pipeline cache.
static SDF_PIPELINE_CACHE: Lazy<RwLock<HashMap<u64, Arc<CachedSdfPipeline>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Cached render targets for a specific resolution.
#[allow(dead_code)]
struct CachedTargets {
    width: u32,
    height: u32,
    output_texture: wgpu::Texture,
    output_view: wgpu::TextureView,
    readback_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
}

/// Process-global render target cache keyed by (width, height).
static TARGET_CACHE: Lazy<RwLock<HashMap<(u32, u32), Arc<CachedTargets>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

fn hash_wgsl(wgsl: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    wgsl.hash(&mut hasher);
    hasher.finish()
}

fn get_or_create_pipeline(device: &wgpu::Device, wgsl: &str) -> Arc<CachedSdfPipeline> {
    let hash = hash_wgsl(wgsl);

    // Check cache
    if let Some(cached) = SDF_PIPELINE_CACHE.read().unwrap().get(&hash) {
        return cached.clone();
    }

    // Cache miss — compile
    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("SDF Live Shader"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(wgsl)),
    });

    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
        ],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("SDF Live Pipeline"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bgl],
                push_constant_ranges: &[],
            }),
        ),
        module: &shader_module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let cached = Arc::new(CachedSdfPipeline { pipeline, bgl });
    SDF_PIPELINE_CACHE
        .write()
        .unwrap()
        .insert(hash, cached.clone());
    cached
}

fn get_or_create_targets(device: &wgpu::Device, width: u32, height: u32) -> Arc<CachedTargets> {
    let key = (width, height);

    if let Some(cached) = TARGET_CACHE.read().unwrap().get(&key) {
        return cached.clone();
    }

    let output_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SDF Live Output"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let padded_row = (width * 4 + 255) & !255;
    let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("SDF Live Readback"),
        size: (padded_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("SDF Live Uniforms"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let cached = Arc::new(CachedTargets {
        width,
        height,
        output_texture,
        output_view,
        readback_buffer,
        uniform_buffer,
    });
    TARGET_CACHE.write().unwrap().insert(key, cached.clone());
    cached
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    resolution: [f32; 2],
    time: f32,
    _pad0: f32,
    camera_pos: [f32; 3],
    _pad1: f32,
    camera_target: [f32; 3],
    fov: f32,
}

fn parse_sdf(msg: Option<&Message>) -> Option<SdfNode> {
    match msg {
        Some(Message::Object(v)) => {
            let json: serde_json::Value = v.as_ref().clone().into();
            serde_json::from_value(json).ok()
        }
        _ => None,
    }
}

#[actor(
    SdfLiveRenderActor,
    inports::<100>(sdf, scene, camera, time),
    outports::<50>(stream, metadata, error),
    state(MemoryState)
)]
pub async fn sdf_live_render_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
    let height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(512) as u32;

    // Cache SDF when it arrives — accepts raw SDF node or pre-composed scene
    if let Some(sdf_msg) = payload.get("sdf").or(payload.get("scene")) {
        if let Some(root) = parse_sdf(Some(sdf_msg)) {
            let settings = SceneSettings {
                width,
                height,
                max_steps: config
                    .get("maxSteps")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(128) as u32,
                fov: config.get("fov").and_then(|v| v.as_f64()).unwrap_or(45.0) as f32,
                camera_pos: [
                    config
                        .get("cameraPosX")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(3.0) as f32,
                    config
                        .get("cameraPosY")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(2.0) as f32,
                    config
                        .get("cameraPosZ")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(4.0) as f32,
                ],
                camera_target: [0.0; 3],
                soft_shadows: config
                    .get("softShadows")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                ao: config.get("ao").and_then(|v| v.as_bool()).unwrap_or(true),
                ..Default::default()
            };
            let scene = root.into_scene_with(settings);
            let compiled = reflow_sdf::codegen::compile(&scene);
            ctx.pool_upsert("_sdf", "wgsl", json!(compiled.wgsl));
            ctx.pool_upsert("_sdf", "node_count", json!(compiled.node_count));
        }
    }

    // Cache camera when it arrives
    if let Some(Message::Object(cam)) = payload.get("camera") {
        let v: serde_json::Value = cam.as_ref().clone().into();
        ctx.pool_upsert("_sdf", "camera", v);
    }

    // Cache time when it arrives
    if let Some(Message::Float(t)) = payload.get("time") {
        ctx.pool_upsert("_sdf", "time", json!(*t));
    }

    // Read cached state
    let cache: HashMap<String, serde_json::Value> = ctx.get_pool("_sdf").into_iter().collect();
    let wgsl = match cache.get("wgsl").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return Ok(HashMap::new()), // No SDF yet
    };

    let time = cache.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let cam = cache.get("camera");
    let camera_pos = cam
        .and_then(|c| c.get("pos"))
        .and_then(|p| p.as_array())
        .map(|a| {
            [
                a.first().and_then(|v| v.as_f64()).unwrap_or(3.0) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(2.0) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(4.0) as f32,
            ]
        })
        .unwrap_or([
            config
                .get("cameraPosX")
                .and_then(|v| v.as_f64())
                .unwrap_or(3.0) as f32,
            config
                .get("cameraPosY")
                .and_then(|v| v.as_f64())
                .unwrap_or(2.0) as f32,
            config
                .get("cameraPosZ")
                .and_then(|v| v.as_f64())
                .unwrap_or(4.0) as f32,
        ]);
    let camera_target = cam
        .and_then(|c| c.get("target"))
        .and_then(|p| p.as_array())
        .map(|a| {
            [
                a.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            ]
        })
        .unwrap_or([0.0; 3]);
    let fov = cam
        .and_then(|c| c.get("fov"))
        .and_then(|v| v.as_f64())
        .unwrap_or(config.get("fov").and_then(|v| v.as_f64()).unwrap_or(45.0)) as f32;

    let mut results = HashMap::new();

    // Get or create stream
    let existing_stream_id = cache.get("stream_id").and_then(|v| v.as_u64());

    let stream_sender = if let Some(stream_id) = existing_stream_id {
        STREAM_REGISTRY.clone_sender(stream_id)
    } else {
        // First invocation — create stream
        let (tx, handle) = ctx.create_stream(
            "stream",
            Some("video/raw-rgba".to_string()),
            None,
            Some(4), // small buffer — backpressure if consumer is slow
        );
        ctx.pool_upsert("_sdf", "stream_id", json!(handle.stream_id));

        let _ = tx.send(StreamFrame::Begin {
            content_type: Some("video/raw-rgba".to_string()),
            size_hint: None,
            metadata: Some(json!({
                "width": width,
                "height": height,
                "fps": 60,
                "format": "RGBA8",
            })),
        });

        results.insert("stream".to_string(), Message::stream_handle(handle));
        Some(tx)
    };

    // Dispatch render + stream send on blocking thread — non-blocking return
    let wgsl_clone = wgsl.clone();
    if let Some(tx) = stream_sender {
        tokio::task::spawn_blocking(move || {
            match render_frame(
                &wgsl_clone,
                width,
                height,
                time,
                camera_pos,
                camera_target,
                fov,
            ) {
                Ok(pixels) => {
                    let _ = tx.send(StreamFrame::Data(Arc::new(pixels)));
                }
                Err(e) => {
                    let _ = tx.send(StreamFrame::Error(e));
                }
            }
        });
    }

    results.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width,
            "height": height,
            "format": "RGBA8",
        }))),
    );
    Ok(results)
}

fn render_frame(
    wgsl: &str,
    width: u32,
    height: u32,
    time: f32,
    camera_pos: [f32; 3],
    camera_target: [f32; 3],
    fov: f32,
) -> Result<Vec<u8>, String> {
    let ctx = &*crate::gpu::context::GPU_CONTEXT;
    let device = ctx.device();
    let queue = ctx.queue();

    // Cached pipeline (reused across frames if WGSL hasn't changed)
    let cached_pipeline = get_or_create_pipeline(device, wgsl);

    // Cached render targets (reused if resolution hasn't changed)
    let targets = get_or_create_targets(device, width, height);

    // Write uniforms — the ONLY per-frame GPU operation
    queue.write_buffer(
        &targets.uniform_buffer,
        0,
        bytemuck::bytes_of(&Uniforms {
            resolution: [width as f32, height as f32],
            time,
            _pad0: 0.0,
            camera_pos,
            _pad1: 0.0,
            camera_target,
            fov,
        }),
    );

    // Create bind group (references cached uniform buffer + texture)
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &cached_pipeline.bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: targets.uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&targets.output_view),
            },
        ],
    });

    // Dispatch compute
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&cached_pipeline.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }

    // Create a fresh readback buffer per frame (avoids mapped-state conflict)
    let padded_row = (width * 4 + 255) & !255;
    let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("SDF Live Readback"),
        size: (padded_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &targets.output_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(std::iter::once(encoder.finish()));

    // Readback
    let slice = readback_buffer.slice(..);
    let (tx, rx) = flume::bounded(1);
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv()
        .map_err(|_| "Map failed".to_string())?
        .map_err(|e| format!("Map: {:?}", e))?;

    let data = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let start = (y * padded_row) as usize;
        let end = start + (width * 4) as usize;
        pixels.extend_from_slice(&data[start..end]);
    }
    drop(data);
    readback_buffer.unmap();

    Ok(pixels)
}
