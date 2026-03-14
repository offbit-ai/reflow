//! SDF Render Actor — compiles SDF IR to WGSL, dispatches via wgpu compute,
//! reads back the rendered texture, and streams it as raw RGBA.
//!
//! The output is a StreamHandle with content_type "image/raw-rgba" that
//! can be piped to ImageEncode, ImageStreamDisplay, or FileSave.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{
    message::EncodableValue,
    stream::StreamFrame,
    ActorContext,
};
use reflow_sdf::ir::{SceneSettings, SdfNode};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

/// Uniform buffer layout matching the WGSL struct.
/// Must be 16-byte aligned for WebGPU.
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
            let json: serde_json::Value = serde_json::to_value(v.as_ref()).ok()?;
            serde_json::from_value(json).ok()
        }
        _ => None,
    }
}

#[actor(
    SdfRenderActor,
    inports::<10>(sdf),
    outports::<1>(stream, metadata, error),
    state(MemoryState)
)]
pub async fn sdf_render_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();

    let root = parse_sdf(payload.get("sdf"))
        .ok_or_else(|| anyhow::anyhow!("Missing SDF IR on sdf port"))?;

    // Build scene settings from config
    let width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
    let height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
    let time = config.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

    let settings = SceneSettings {
        width,
        height,
        max_steps: config.get("maxSteps").and_then(|v| v.as_u64()).unwrap_or(128) as u32,
        fov: config.get("fov").and_then(|v| v.as_f64()).unwrap_or(45.0) as f32,
        camera_pos: [
            config.get("cameraPosX").and_then(|v| v.as_f64()).unwrap_or(3.0) as f32,
            config.get("cameraPosY").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32,
            config.get("cameraPosZ").and_then(|v| v.as_f64()).unwrap_or(4.0) as f32,
        ],
        camera_target: [
            config.get("cameraTargetX").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            config.get("cameraTargetY").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            config.get("cameraTargetZ").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        ],
        soft_shadows: config.get("softShadows").and_then(|v| v.as_bool()).unwrap_or(false),
        ao: config.get("ao").and_then(|v| v.as_bool()).unwrap_or(true),
        ambient: config.get("ambient").and_then(|v| v.as_f64()).unwrap_or(0.15) as f32,
        time,
        ..Default::default()
    };

    let scene = root.into_scene_with(settings.clone());
    let compiled = reflow_sdf::codegen::compile(&scene);
    let shader_source = compiled.wgsl;

    let total_bytes = (width * height * 4) as u64;

    let (tx, handle) = context.create_stream(
        "stream",
        Some("image/raw-rgba".to_string()),
        Some(total_bytes),
        None,
    );

    let shader_size = shader_source.len();
    let node_count = compiled.node_count;

    // Run rendering inline — blocks until GPU dispatch + readback completes.
    // No spawn_stream_task: we want the actor to return only after the
    // stream is fully written, avoiding idle background tasks.
    if let Err(e) = render_sdf_to_stream(
        &shader_source,
        width,
        height,
        time,
        settings.camera_pos,
        settings.camera_target,
        settings.fov,
        &tx,
    )
    .await
    {
        let _ = tx
            .send_async(StreamFrame::Error(format!("Render error: {}", e)))
            .await;
    }

    let mut results = HashMap::new();
    results.insert("stream".to_string(), Message::stream_handle(handle));
    results.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width,
            "height": height,
            "shaderSize": shader_size,
            "nodeCount": node_count,
        }))),
    );
    Ok(results)
}

async fn render_sdf_to_stream(
    shader_source: &str,
    width: u32,
    height: u32,
    time: f32,
    camera_pos: [f32; 3],
    camera_target: [f32; 3],
    fov: f32,
    tx: &flume::Sender<StreamFrame>,
) -> Result<(), String> {
    // 1. Initialize wgpu
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

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("SDF Render"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
        }, None)
        .await
        .map_err(|e| format!("Device request failed: {}", e))?;

    // 2. Create output texture
    let output_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SDF Output"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // 3. Create uniform buffer
    let uniforms = Uniforms {
        resolution: [width as f32, height as f32],
        time,
        _pad0: 0.0,
        camera_pos,
        _pad1: 0.0,
        camera_target,
        fov,
    };

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Uniforms"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    // 4. Compile shader
    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("SDF Shader"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
    });

    // 5. Create bind group layout + pipeline
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SDF Bind Group Layout"),
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

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("SDF Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("SDF Compute Pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader_module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("SDF Bind Group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&output_view),
            },
        ],
    });

    // 6. Dispatch compute shader
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("SDF Dispatch"),
    });

    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("SDF Pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&compute_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        // Workgroup size is 8x8 in the shader
        pass.dispatch_workgroups((width + 7) / 8, (height + 7) / 8, 1);
    }

    // 7. Copy texture to readback buffer
    let bytes_per_row = width * 4;
    // WebGPU requires 256-byte aligned rows
    let padded_bytes_per_row = (bytes_per_row + 255) & !255;

    let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("SDF Readback"),
        size: (padded_bytes_per_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &output_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );

    queue.submit(std::iter::once(encoder.finish()));

    // 8. Map buffer and read back pixels
    let buffer_slice = readback_buffer.slice(..);
    let (map_tx, map_rx) = flume::bounded(1);
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = map_tx.send(result);
    });
    device.poll(wgpu::Maintain::Wait);

    map_rx
        .recv_async()
        .await
        .map_err(|_| "Map channel closed")?
        .map_err(|e| format!("Buffer map failed: {:?}", e))?;

    let data = buffer_slice.get_mapped_range();

    // 9. Stream the pixels
    let _ = tx
        .send_async(StreamFrame::Begin {
            content_type: Some("image/raw-rgba".to_string()),
            size_hint: Some((width * height * 4) as u64),
            metadata: Some(json!({
                "width": width,
                "height": height,
                "format": "RGBA8",
                "channels": 4,
            })),
        })
        .await;

    // Stream row by row (removing padding)
    for y in 0..height {
        let start = (y * padded_bytes_per_row) as usize;
        let end = start + (width * 4) as usize;
        let row = &data[start..end];
        if tx
            .send_async(StreamFrame::Data(Arc::new(row.to_vec())))
            .await
            .is_err()
        {
            break;
        }
    }

    let _ = tx.send_async(StreamFrame::End).await;
    drop(data);
    readback_buffer.unmap();

    Ok(())
}

/// Extension trait for creating initialized buffers.
use wgpu::util::DeviceExt;
