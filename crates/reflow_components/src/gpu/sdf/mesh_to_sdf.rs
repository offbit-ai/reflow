//! GPU Mesh → SDF Actor — generates a signed distance field from triangle mesh.
//!
//! Uploads mesh triangles to GPU, dispatches compute shader that evaluates
//! distance to nearest triangle for each voxel in a 3D grid. Output is a
//! 3D distance field stored as f32 bytes (resolution³ values).
//!
//! The distance field can be used as a custom SDF primitive in downstream
//! SDF operations (union, difference, transforms, etc.).

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::json;
use std::collections::HashMap;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshToSdfUniforms {
    resolution: u32,
    triangle_count: u32,
    _pad0: [u32; 2],
    bound_min: [f32; 3],
    _pad1: f32,
    bound_max: [f32; 3],
    _pad2: f32,
}

#[actor(
    MeshToSdfActor,
    inports::<10>(mesh),
    outports::<1>(output, metadata, error),
    state(MemoryState)
)]
pub async fn mesh_to_sdf_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let bytes = match payload.get("mesh") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on mesh port")),
    };

    let resolution = config
        .get("resolution")
        .and_then(|v| v.as_u64())
        .unwrap_or(32) as u32;
    let bound = config.get("bound").and_then(|v| v.as_f64()).unwrap_or(3.0) as f32;
    let stride = config.get("stride").and_then(|v| v.as_u64()).unwrap_or(24) as usize;

    let floats_per_vertex = stride / 4;
    let float_data: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    let vertex_count = float_data.len() / floats_per_vertex;
    let triangle_count = (vertex_count / 3) as u32;

    if triangle_count == 0 {
        return Ok(error_output("No triangles in mesh"));
    }

    // Extract just positions (first 3 floats per vertex)
    let mut positions: Vec<f32> = Vec::with_capacity(vertex_count * 3);
    for i in 0..vertex_count {
        let base = i * floats_per_vertex;
        positions.push(float_data[base]);
        positions.push(float_data[base + 1]);
        positions.push(float_data[base + 2]);
    }

    // Pad to vec4 alignment for GPU (3 floats + 1 pad per vertex)
    let mut padded_positions: Vec<f32> = Vec::with_capacity(vertex_count * 4);
    for i in 0..vertex_count {
        padded_positions.push(positions[i * 3]);
        padded_positions.push(positions[i * 3 + 1]);
        padded_positions.push(positions[i * 3 + 2]);
        padded_positions.push(0.0); // padding
    }

    let sdf_data = tokio::task::spawn_blocking(move || {
        run_mesh_to_sdf_gpu(&padded_positions, triangle_count, resolution, bound)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Spawn failed: {}", e))?
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    let voxel_count = (resolution * resolution * resolution) as usize;

    let mut results = HashMap::new();
    results.insert("output".to_string(), Message::bytes(sdf_data));
    results.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "resolution": resolution,
            "bound": bound,
            "voxelCount": voxel_count,
            "triangleCount": triangle_count,
            "dataType": "f32",
            "format": "sdf_volume",
        }))),
    );
    Ok(results)
}

fn run_mesh_to_sdf_gpu(
    padded_positions: &[f32],
    triangle_count: u32,
    resolution: u32,
    bound: f32,
) -> Result<Vec<u8>, String> {
    use wgpu::util::DeviceExt;

    let ctx = &*crate::gpu::context::GPU_CONTEXT;
    let device = ctx.device();
    let queue = ctx.queue();

    let voxel_count = (resolution * resolution * resolution) as u64;
    let sdf_buf_size = voxel_count * 4; // f32 per voxel

    let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("M2S Uniforms"),
        contents: bytemuck::bytes_of(&MeshToSdfUniforms {
            resolution,
            triangle_count,
            _pad0: [0; 2],
            bound_min: [-bound, -bound, -bound],
            _pad1: 0.0,
            bound_max: [bound, bound, bound],
            _pad2: 0.0,
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let tri_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Triangles"),
        contents: bytemuck::cast_slice(padded_positions),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let sdf_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("SDF Volume"),
        size: sdf_buf_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let shader_source = generate_mesh_to_sdf_wgsl();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("M2S Shader"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Owned(shader_source)),
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
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("M2S Pipeline"),
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
                resource: tri_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: sdf_buf.as_entire_binding(),
            },
        ],
    });

    let wg = resolution.div_ceil(4);
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

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("SDF Readback"),
        size: sdf_buf_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_buffer_to_buffer(&sdf_buf, 0, &readback, 0, sdf_buf_size);
    queue.submit(std::iter::once(encoder.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = flume::bounded(1);
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv()
        .map_err(|_| "Map failed".to_string())?
        .map_err(|e| format!("Map: {:?}", e))?;

    let data = slice.get_mapped_range();
    let result = data.to_vec();
    drop(data);
    readback.unmap();

    Ok(result)
}

fn generate_mesh_to_sdf_wgsl() -> String {
    r#"
struct Uniforms {
  resolution: u32,
  triangle_count: u32,
  _pad0: vec2u,
  bound_min: vec3f,
  _pad1: f32,
  bound_max: vec3f,
  _pad2: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<storage, read> triangles: array<vec4f>;
@group(0) @binding(2) var<storage, read_write> sdf_volume: array<f32>;

// Unsigned distance from point to triangle
fn point_triangle_dist(p: vec3f, a: vec3f, b: vec3f, c: vec3f) -> f32 {
  let ba = b - a; let pa = p - a;
  let cb = c - b; let pb = p - b;
  let ac = a - c; let pc = p - c;
  let nor = cross(ba, ac);

  let sign_check = sign(dot(cross(ba, nor), pa)) +
                   sign(dot(cross(cb, nor), pb)) +
                   sign(dot(cross(ac, nor), pc));

  if sign_check < 2.0 {
    // Closest point is on an edge or vertex
    let d1 = dot(ba, pa) / dot(ba, ba);
    let e1 = pa - ba * clamp(d1, 0.0, 1.0);
    let d2 = dot(cb, pb) / dot(cb, cb);
    let e2 = pb - cb * clamp(d2, 0.0, 1.0);
    let d3 = dot(ac, pc) / dot(ac, ac);
    let e3 = pc - ac * clamp(d3, 0.0, 1.0);
    return sqrt(min(min(dot(e1, e1), dot(e2, e2)), dot(e3, e3)));
  } else {
    // Closest point is on the face
    return abs(dot(nor, pa)) / length(nor);
  }
}

@compute @workgroup_size(4, 4, 4)
fn main(@builtin(global_invocation_id) gid: vec3u) {
  let res = u.resolution;
  if gid.x >= res || gid.y >= res || gid.z >= res { return; }

  let step = (u.bound_max - u.bound_min) / f32(res);
  let p = u.bound_min + (vec3f(f32(gid.x), f32(gid.y), f32(gid.z)) + 0.5) * step;

  var min_dist = 1e10;

  for (var i = 0u; i < u.triangle_count; i = i + 1u) {
    let a = triangles[i * 3u].xyz;
    let b = triangles[i * 3u + 1u].xyz;
    let c = triangles[i * 3u + 2u].xyz;
    let d = point_triangle_dist(p, a, b, c);
    min_dist = min(min_dist, d);
  }

  // Simple sign estimation: use winding number approximation
  // For proper inside/outside, we'd need ray casting. For now,
  // use unsigned distance (usable for shell operations).
  let idx = gid.z * res * res + gid.y * res + gid.x;
  sdf_volume[idx] = min_dist;
}
"#
    .to_string()
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
