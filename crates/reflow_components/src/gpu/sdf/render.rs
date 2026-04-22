//! SDF Render Actor — compiles SDF IR to WGSL, dispatches via wgpu compute,
//! reads back the rendered texture, and outputs raw RGBA pixels as Message::Bytes.
//!
//! Output ports:
//! - `output`: Message::Bytes — raw RGBA pixels (width × height × 4)
//! - `metadata`: width, height, shader stats
//! - `error`: on failure

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use reflow_sdf::ir::{SceneSettings, SdfNode};
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Uniform buffer layout matching the WGSL struct.
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

fn render_config_signature(
    config: &HashMap<String, serde_json::Value>,
    width: u32,
    height: u32,
) -> serde_json::Value {
    json!({
        "width": width,
        "height": height,
        "maxSteps": config.get("maxSteps").cloned().unwrap_or(json!(128)),
        "fov": config.get("fov").cloned().unwrap_or(json!(45.0)),
        "cameraPosX": config.get("cameraPosX").cloned().unwrap_or(json!(3.0)),
        "cameraPosY": config.get("cameraPosY").cloned().unwrap_or(json!(2.0)),
        "cameraPosZ": config.get("cameraPosZ").cloned().unwrap_or(json!(4.0)),
        "cameraTargetX": config.get("cameraTargetX").cloned().unwrap_or(json!(0.0)),
        "cameraTargetY": config.get("cameraTargetY").cloned().unwrap_or(json!(0.0)),
        "cameraTargetZ": config.get("cameraTargetZ").cloned().unwrap_or(json!(0.0)),
        "softShadows": config.get("softShadows").cloned().unwrap_or(json!(false)),
        "ao": config.get("ao").cloned().unwrap_or(json!(true)),
        "ambient": config.get("ambient").cloned().unwrap_or(json!(0.15)),
        "shadowK": config.get("shadowK").cloned().unwrap_or(json!(32.0)),
        "lightDir": config
            .get("lightDir")
            .cloned()
            .unwrap_or(json!([0.577, 0.577, -0.577])),
        "lightColor": config
            .get("lightColor")
            .cloned()
            .unwrap_or(json!([1.0, 1.0, 1.0])),
        "background": config
            .get("background")
            .cloned()
            .unwrap_or(json!([0.1, 0.1, 0.15])),
    })
}

fn wgsl_hash_hex(wgsl: &str) -> String {
    let mut hasher = DefaultHasher::new();
    wgsl.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn wgsl_probe_snippet<'a>(wgsl: &'a str, marker: &str) -> Option<&'a str> {
    let start = wgsl.find(marker)?;
    let end = wgsl[start..]
        .find('\n')
        .map(|idx| start + idx)
        .unwrap_or(wgsl.len());
    Some(wgsl[start..end].trim())
}

#[actor(
    SdfRenderActor,
    inports::<10>(sdf, time),
    outports::<1>(output, metadata, error),
    state(MemoryState)
)]
pub async fn sdf_render_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let log_progress = config
        .get("logProgress")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut sdf_updated = false;
    // Cache SDF on first receipt, re-render on time updates
    if let Some(sdf_msg) = payload.get("sdf") {
        if let Some(root) = parse_sdf(Some(sdf_msg)) {
            let ir_json = serde_json::to_value(&root).unwrap_or(serde_json::json!(null));
            context.pool_upsert("_sdf_render", "ir", ir_json);
            sdf_updated = true;
        }
    }

    let cache: HashMap<String, serde_json::Value> =
        context.get_pool("_sdf_render").into_iter().collect();
    let render_count = cache
        .get("render_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        + 1;
    context.pool_upsert("_sdf_render", "render_count", json!(render_count));
    let root: SdfNode = match cache.get("ir") {
        Some(v) => {
            serde_json::from_value(v.clone()).map_err(|e| anyhow::anyhow!("SDF IR: {}", e))?
        }
        None => return Ok(HashMap::new()),
    };

    let width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
    let height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
    let config_signature = render_config_signature(&config, width, height);
    let config_changed = cache
        .get("config_signature")
        .map(|sig| sig != &config_signature)
        .unwrap_or(true);
    let time = match payload.get("time") {
        Some(Message::Float(f)) => *f as f32,
        Some(Message::Integer(i)) => *i as f32,
        _ => config.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
    };
    let has_custom_shade = !extract_custom_shade(&root).is_empty();

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
        camera_target: [
            config
                .get("cameraTargetX")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32,
            config
                .get("cameraTargetY")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32,
            config
                .get("cameraTargetZ")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32,
        ],
        soft_shadows: config
            .get("softShadows")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        ao: config.get("ao").and_then(|v| v.as_bool()).unwrap_or(true),
        ambient: config
            .get("ambient")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.15) as f32,
        shadow_k: config
            .get("shadowK")
            .and_then(|v| v.as_f64())
            .unwrap_or(32.0) as f32,
        light_dir: config
            .get("lightDir")
            .and_then(|v| v.as_array())
            .map(|a| {
                [
                    a.get(0).and_then(|v| v.as_f64()).unwrap_or(0.577) as f32,
                    a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.577) as f32,
                    a.get(2).and_then(|v| v.as_f64()).unwrap_or(-0.577) as f32,
                ]
            })
            .unwrap_or([0.577, 0.577, -0.577]),
        light_color: config
            .get("lightColor")
            .and_then(|v| v.as_array())
            .map(|a| {
                [
                    a.get(0).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                    a.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                    a.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                ]
            })
            .unwrap_or([1.0, 1.0, 1.0]),
        background: config
            .get("background")
            .and_then(|v| v.as_array())
            .map(|a| {
                [
                    a.get(0).and_then(|v| v.as_f64()).unwrap_or(0.1) as f32,
                    a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.1) as f32,
                    a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.15) as f32,
                ]
            })
            .unwrap_or([0.1, 0.1, 0.15]),
        time,
        // Preserve custom shade from upstream SdfScene if present
        custom_shade_wgsl: extract_custom_shade(&root),
        ..Default::default()
    };
    let max_steps = settings.max_steps;

    let cached_wgsl = if sdf_updated || config_changed {
        None
    } else {
        cache
            .get("wgsl")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
    };
    let using_cached_wgsl = cached_wgsl.is_some();
    let cached_shader_size = cache
        .get("shader_size")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    let cached_node_count = cache
        .get("node_count")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let (wgsl, shader_size, node_count) = if let (Some(wgsl), Some(shader_size), Some(node_count)) =
        (cached_wgsl, cached_shader_size, cached_node_count)
    {
        (wgsl, shader_size, node_count)
    } else {
        let scene = root.into_scene_with(settings);
        let compiled = reflow_sdf::codegen::compile(&scene);
        let shader_size = compiled.wgsl.len();
        let node_count = compiled.node_count;
        context.pool_upsert("_sdf_render", "wgsl", json!(compiled.wgsl));
        context.pool_upsert("_sdf_render", "shader_size", json!(shader_size));
        context.pool_upsert("_sdf_render", "node_count", json!(node_count));
        context.pool_upsert("_sdf_render", "config_signature", config_signature);
        (compiled.wgsl, shader_size, node_count)
    };

    if log_progress && (render_count <= 3 || render_count % 6 == 0) {
        eprintln!(
            "[sdf_render] start frame={} time={:.3} cached_wgsl={} steps={} {}x{}",
            render_count,
            time,
            using_cached_wgsl && !sdf_updated,
            max_steps,
            width,
            height
        );
        eprintln!(
            "[sdf_render] wgsl_hash={} custom_shade={} reflection={} absorption={}",
            wgsl_hash_hex(&wgsl),
            if has_custom_shade { "yes" } else { "no" },
            wgsl_probe_snippet(&wgsl, "let reflection_weight =").unwrap_or("n/a"),
            wgsl_probe_snippet(&wgsl, "let absorption =").unwrap_or("n/a"),
        );
    }

    // Run GPU render on a blocking thread to avoid stalling the tokio runtime
    let pixels = tokio::task::spawn_blocking(move || {
        render_to_pixels(
            &wgsl,
            width,
            height,
            time,
            [
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
            [
                config
                    .get("cameraTargetX")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32,
                config
                    .get("cameraTargetY")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32,
                config
                    .get("cameraTargetZ")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32,
            ],
            config.get("fov").and_then(|v| v.as_f64()).unwrap_or(45.0) as f32,
        )
    })
    .await
    .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    if log_progress && (render_count <= 3 || render_count % 6 == 0) {
        eprintln!(
            "[sdf_render] done frame={} shader={} nodes={}",
            render_count, shader_size, node_count
        );
    }

    let mut results = HashMap::new();
    results.insert("output".to_string(), Message::bytes(pixels));
    results.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width,
            "height": height,
            "format": "RGBA8",
            "channels": 4,
            "shaderSize": shader_size,
            "nodeCount": node_count,
        }))),
    );
    Ok(results)
}

/// Extract custom_shade_wgsl from an incoming SdfNode if it's a Scene with custom shade.
fn extract_custom_shade(node: &SdfNode) -> String {
    match node {
        SdfNode::Scene { settings, .. } => settings.custom_shade_wgsl.clone(),
        _ => String::new(),
    }
}

/// Synchronous GPU render — runs on a blocking thread.
fn render_to_pixels(
    shader_source: &str,
    width: u32,
    height: u32,
    time: f32,
    camera_pos: [f32; 3],
    camera_target: [f32; 3],
    fov: f32,
) -> Result<Vec<u8>, String> {
    use wgpu::util::DeviceExt;

    let ctx = crate::gpu::context::try_gpu_context()?;
    let device = ctx.device();
    let queue = ctx.queue();

    let output_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SDF Output"),
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

    eprintln!(
        "[sdf_render] cam=[{:.1},{:.1},{:.1}] target=[{:.1},{:.1},{:.1}] fov={:.0} time={:.2}",
        camera_pos[0],
        camera_pos[1],
        camera_pos[2],
        camera_target[0],
        camera_target[1],
        camera_target[2],
        fov,
        time
    );
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Uniforms"),
        contents: bytemuck::bytes_of(&Uniforms {
            resolution: [width as f32, height as f32],
            time,
            _pad0: 0.0,
            camera_pos,
            _pad1: 0.0,
            camera_target,
            fov,
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("SDF Shader"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
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
        label: Some("SDF Pipeline"),
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

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bgl,
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

    // Dispatch
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }

    // Readback
    let padded_row = (width * 4 + 255) & !255;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Readback"),
        size: (padded_row * height) as u64,
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
            buffer: &readback,
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

    let slice = readback.slice(..);
    let (tx, rx) = flume::bounded(1);
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv()
        .map_err(|_| "Map channel closed".to_string())?
        .map_err(|e| format!("Buffer map failed: {:?}", e))?;

    let data = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let start = (y * padded_row) as usize;
        let end = start + (width * 4) as usize;
        pixels.extend_from_slice(&data[start..end]);
    }
    drop(data);
    readback.unmap();

    Ok(pixels)
}
