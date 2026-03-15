//! GPU Marching Cubes Actor — extracts triangle mesh from SDF via compute shader.
//!
//! Takes compiled SDF IR, generates a WGSL marching cubes shader,
//! dispatches on GPU, reads back vertex buffer (pos3+normal3 per vertex).

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_sdf::ir::{SceneSettings, SdfNode};
use serde_json::json;
use std::collections::HashMap;

fn parse_sdf(msg: Option<&Message>) -> Option<SdfNode> {
    match msg {
        Some(Message::Object(v)) => {
            let json: serde_json::Value = v.as_ref().clone().into();
            serde_json::from_value(json).ok()
        }
        _ => None,
    }
}

/// Uniform buffer for marching cubes.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct MCUniforms {
    resolution: u32,
    _pad0: [u32; 3],
    bound_min: [f32; 3],
    _pad1: f32,
    bound_max: [f32; 3],
    iso_level: f32,
}

#[actor(
    SdfMarchingCubesActor,
    inports::<10>(sdf),
    outports::<1>(mesh, metadata, error),
    state(MemoryState)
)]
pub async fn sdf_marching_cubes_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let root = parse_sdf(payload.get("sdf"))
        .ok_or_else(|| anyhow::anyhow!("Missing SDF IR on sdf port"))?;

    let resolution = config
        .get("resolution")
        .and_then(|v| v.as_u64())
        .unwrap_or(64) as u32;
    let bound = config.get("bound").and_then(|v| v.as_f64()).unwrap_or(3.0) as f32;
    let iso_level = config
        .get("isoLevel")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;

    // Compile SDF to WGSL
    let scene = root.into_scene_with(SceneSettings::default());
    let compiled = reflow_sdf::codegen::compile(&scene);

    // Generate marching cubes WGSL
    let mc_wgsl =
        reflow_sdf::marching_cubes::generate_marching_cubes_wgsl(&compiled.wgsl, resolution);

    // Run on blocking thread (wgpu uses pollster)
    let mesh_data = tokio::task::spawn_blocking(move || {
        run_marching_cubes_gpu(&mc_wgsl, resolution, bound, iso_level)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Spawn failed: {}", e))?
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    let vertex_count = mesh_data.len() / (6 * 4); // 6 floats per vertex, 4 bytes per float
    let triangle_count = vertex_count / 3;

    let mut results = HashMap::new();
    results.insert("mesh".to_string(), Message::bytes(mesh_data));
    results.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "vertexCount": vertex_count,
            "triangleCount": triangle_count,
            "resolution": resolution,
            "bound": bound,
            "isoLevel": iso_level,
            "format": "pos3_normal3_f32",
            "stride": 24, // 6 floats × 4 bytes
        }))),
    );
    Ok(results)
}

fn run_marching_cubes_gpu(
    shader_source: &str,
    resolution: u32,
    bound: f32,
    iso_level: f32,
) -> Result<Vec<u8>, String> {
    use wgpu::util::DeviceExt;

    let ctx = &*crate::gpu::context::GPU_CONTEXT;
    let device = ctx.device();
    let queue = ctx.queue();

    // Allocate vertex buffer: worst-case is (res-1)^3 × 15 vertices, but real SDFs
    // only produce surface triangles on a thin shell. Cap at 32MB to avoid GPU OOM.
    let max_cells = (resolution - 1) as u64 * (resolution - 1) as u64 * (resolution - 1) as u64;
    let worst_case = max_cells * 15 * 6 * 4; // bytes
    let max_buf: u64 = 32 * 1024 * 1024; // 32 MB cap
    let vertex_buf_size = worst_case.min(max_buf);

    // Uniforms
    let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("MC Uniforms"),
        contents: bytemuck::bytes_of(&MCUniforms {
            resolution,
            _pad0: [0; 3],
            bound_min: [-bound, -bound, -bound],
            _pad1: 0.0,
            bound_max: [bound, bound, bound],
            iso_level,
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    // Edge table buffer
    let edge_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Edge Table"),
        contents: bytemuck::cast_slice(&reflow_sdf::marching_cubes::EDGE_TABLE),
        usage: wgpu::BufferUsages::STORAGE,
    });

    // Triangle table buffer
    let tri_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Tri Table"),
        contents: bytemuck::cast_slice(&reflow_sdf::marching_cubes::TRI_TABLE),
        usage: wgpu::BufferUsages::STORAGE,
    });

    // Vertex output buffer
    let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Vertices"),
        size: vertex_buf_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    // Atomic counter buffer (single u32)
    let counter_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Counter"),
        contents: &[0u8; 4],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });

    // Compile shader
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("MC Shader"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
    });

    // Pipeline
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
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("MC Pipeline"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bgl],
                push_constant_ranges: &[],
            }),
        ),
        module: &shader,
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
                resource: uniform_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: edge_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: tri_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: vertex_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: counter_buf.as_entire_binding(),
            },
        ],
    });

    // Dispatch
    let wg = (resolution + 3) / 4; // workgroup size is 4×4×4
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(wg, wg, wg);
    }

    // Read back counter
    let counter_readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Counter Readback"),
        size: 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_buffer_to_buffer(&counter_buf, 0, &counter_readback, 0, 4);

    queue.submit(std::iter::once(encoder.finish()));

    // Map counter
    let slice = counter_readback.slice(..);
    let (tx, rx) = flume::bounded(1);
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv()
        .map_err(|_| "Counter map failed".to_string())?
        .map_err(|e| format!("Counter map: {:?}", e))?;

    let counter_data = slice.get_mapped_range();
    let float_count = u32::from_le_bytes([
        counter_data[0],
        counter_data[1],
        counter_data[2],
        counter_data[3],
    ]) as u64;
    drop(counter_data);
    counter_readback.unmap();

    if float_count == 0 {
        return Ok(Vec::new());
    }

    let byte_count = float_count * 4;

    // Read back vertices
    let vertex_readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Vertex Readback"),
        size: byte_count,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder2 =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder2.copy_buffer_to_buffer(&vertex_buf, 0, &vertex_readback, 0, byte_count);
    queue.submit(std::iter::once(encoder2.finish()));

    let slice2 = vertex_readback.slice(..);
    let (tx2, rx2) = flume::bounded(1);
    slice2.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx2.send(r);
    });
    device.poll(wgpu::Maintain::Wait);
    rx2.recv()
        .map_err(|_| "Vertex map failed".to_string())?
        .map_err(|e| format!("Vertex map: {:?}", e))?;

    let vertex_data = slice2.get_mapped_range();
    let result = vertex_data.to_vec();
    drop(vertex_data);
    vertex_readback.unmap();

    Ok(result)
}
