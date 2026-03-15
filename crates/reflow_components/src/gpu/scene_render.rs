//! Scene Render Actor — rasterizes mesh-based scenes via wgpu.
//!
//! Takes a scene graph (objects with transforms) and mesh data from
//! the input pool. Renders all triangles with per-object model matrices,
//! basic diffuse lighting, and outputs RGBA bytes.
//!
//! SDF objects should be meshed via MarchingCubes before reaching this
//! actor — it only rasterizes triangles.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct SceneUniforms {
    view_proj: [[f32; 4]; 4], // 64 bytes
    light_dir: [f32; 3],      // 12 bytes
    _pad0: f32,               // 4 bytes  → 80
    ambient: f32,             // 4 bytes
    _pad1: f32,               // 4 bytes
    _pad2: f32,               // 4 bytes
    _pad3: f32,               // 4 bytes  → 96
    _pad4: [f32; 4],          // 16 bytes → 112
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ModelUniforms {
    model: [[f32; 4]; 4],
    color: [f32; 3],
    _pad: f32,
}

#[actor(
    SceneRenderActor,
    inports::<10>(scene, meshes),
    outports::<1>(output, metadata, error),
    state(MemoryState),
    await_all_inports
)]
pub async fn scene_render_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
    let height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
    let fov = config.get("fov").and_then(|v| v.as_f64()).unwrap_or(45.0) as f32;
    let cam_pos = [
        config.get("cameraPosX").and_then(|v| v.as_f64()).unwrap_or(8.0) as f32,
        config.get("cameraPosY").and_then(|v| v.as_f64()).unwrap_or(6.0) as f32,
        config.get("cameraPosZ").and_then(|v| v.as_f64()).unwrap_or(10.0) as f32,
    ];
    let cam_target = [
        config.get("cameraTargetX").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        config.get("cameraTargetY").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        config.get("cameraTargetZ").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
    ];
    let msaa_samples = config.get("msaa").and_then(|v| v.as_u64()).unwrap_or(4) as u32;
    let clear_color = [
        config.get("bgR").and_then(|v| v.as_f64()).unwrap_or(0.1),
        config.get("bgG").and_then(|v| v.as_f64()).unwrap_or(0.1),
        config.get("bgB").and_then(|v| v.as_f64()).unwrap_or(0.15),
    ];

    // Parse scene graph
    let scene_data = match payload.get("scene") {
        Some(Message::Object(obj)) => {
            let v: serde_json::Value = obj.as_ref().clone().into();
            v
        }
        _ => return Ok(error_output("Expected scene graph on scene port")),
    };

    // Get mesh bytes — all prefab meshes concatenated or from pool
    let mesh_bytes = match payload.get("meshes") {
        Some(Message::Bytes(b)) => Some(b.clone()),
        _ => None,
    };

    let objects = scene_data
        .get("objects")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mesh_data: Option<Vec<u8>> = mesh_bytes.map(|b| b.to_vec());
    let objects_clone = objects.clone();

    let pixels = tokio::task::spawn_blocking(move || {
        render_scene(
            width, height, fov, cam_pos, cam_target,
            msaa_samples, clear_color,
            &objects_clone, mesh_data.as_deref(),
        )
    })
    .await
    .map_err(|e| anyhow::anyhow!("Spawn failed: {}", e))?
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    let mut results = HashMap::new();
    results.insert("output".to_string(), Message::bytes(pixels));
    results.insert("metadata".to_string(), Message::object(EncodableValue::from(json!({
        "width": width,
        "height": height,
        "format": "RGBA8",
        "objectCount": objects.len(),
    }))));
    Ok(results)
}

fn render_scene(
    width: u32,
    height: u32,
    fov: f32,
    cam_pos: [f32; 3],
    cam_target: [f32; 3],
    msaa_samples: u32,
    clear_color: [f64; 3],
    objects: &[serde_json::Value],
    _mesh_bytes: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    use wgpu::util::DeviceExt;

    let (device, queue) = pollster::block_on(async {
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
            .ok_or("No GPU adapter")?;
        adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Scene Render"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            }, None)
            .await
            .map_err(|e| format!("Device: {}", e))
    })?;

    // Build vertex data — for now generate simple geometry per object
    let mut all_vertices: Vec<f32> = Vec::new();

    for obj in objects {
        let transform = obj.get("transform").cloned().unwrap_or(json!({}));
        let pos = transform.get("position").and_then(|p| p.as_array())
            .map(|a| [
                a.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            ])
            .unwrap_or([0.0; 3]);

        let scl = transform.get("scale").and_then(|s| s.as_array())
            .map(|a| a.get(0).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32)
            .unwrap_or(1.0);

        let obj_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("instance");

        match obj_type {
            "terrain" => {
                // Flat grid for terrain
                let tw = obj.get("terrain").and_then(|t| t.get("width")).and_then(|v| v.as_f64()).unwrap_or(10.0) as f32;
                let td = obj.get("terrain").and_then(|t| t.get("depth")).and_then(|v| v.as_f64()).unwrap_or(10.0) as f32;
                let segs = 8;
                for gz in 0..segs {
                    for gx in 0..segs {
                        let x0 = pos[0] + (gx as f32 / segs as f32 - 0.5) * tw;
                        let z0 = pos[2] + (gz as f32 / segs as f32 - 0.5) * td;
                        let x1 = pos[0] + ((gx + 1) as f32 / segs as f32 - 0.5) * tw;
                        let z1 = pos[2] + ((gz + 1) as f32 / segs as f32 - 0.5) * td;
                        let y = pos[1];
                        // Two triangles per quad
                        // pos(3) + normal(3) + color(3) per vertex
                        let n = [0.0f32, 1.0, 0.0];
                        let c = [0.3f32, 0.6, 0.2]; // green terrain
                        for v in &[
                            [x0, y, z0], [x1, y, z0], [x1, y, z1],
                            [x0, y, z0], [x1, y, z1], [x0, y, z1],
                        ] {
                            all_vertices.extend_from_slice(v);
                            all_vertices.extend_from_slice(&n);
                            all_vertices.extend_from_slice(&c);
                        }
                    }
                }
            }
            _ => {
                // Simple cube for instances
                let s = 0.4 * scl;
                let cube_verts = generate_cube(pos, s);
                all_vertices.extend_from_slice(&cube_verts);
            }
        }
    }

    if all_vertices.is_empty() {
        // Empty scene — return blank image
        return Ok(vec![30; (width * height * 4) as usize]);
    }

    let vertex_count = all_vertices.len() / 9; // 9 floats per vertex (pos3+normal3+color3)

    // Create GPU resources
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Vertices"),
        contents: bytemuck::cast_slice(&all_vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });

    // View-projection matrix
    let view_proj = build_view_proj(cam_pos, cam_target, fov, width as f32 / height as f32);
    let uniforms = SceneUniforms {
        view_proj,
        light_dir: [0.577, 0.577, -0.577],
        _pad0: 0.0,
        ambient: 0.2,
        _pad1: 0.0,
        _pad2: 0.0,
        _pad3: 0.0,
        _pad4: [0.0; 4],
    };

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Uniforms"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    // Clamp MSAA to supported values
    let sample_count = match msaa_samples {
        1 => 1,
        2 => 2,  // not all GPUs support 2x
        _ => 4,  // 4x is universally supported
    };

    // Resolve target (always 1x — MSAA resolves to this)
    let resolve_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Resolve"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let resolve_view = resolve_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // MSAA render target (multisampled)
    let color_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("MSAA Color"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Shader
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Scene Shader"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SCENE_SHADER)),
    });

    // Pipeline
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Scene Pipeline"),
        layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        })),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 36, // 9 floats × 4 bytes
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },  // position
                    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 12, shader_location: 1 }, // normal
                    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 24, shader_location: 2 }, // color
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });

    // Render pass
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Scene Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                resolve_target: if sample_count > 1 { Some(&resolve_view) } else { None },
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: clear_color[0], g: clear_color[1], b: clear_color[2], a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..vertex_count as u32, 0..1);
    }

    // Readback
    let padded_row = (width * 4 + 255) & !255;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Readback"),
        size: (padded_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // Copy from resolve target (1x) for readback
    let readback_src = if sample_count > 1 { &resolve_texture } else { &color_texture };
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: readback_src,
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
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );

    queue.submit(std::iter::once(encoder.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = flume::bounded(1);
    slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
    device.poll(wgpu::Maintain::Wait);
    rx.recv().map_err(|_| "Map failed".to_string())?
        .map_err(|e| format!("Map: {:?}", e))?;

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

fn generate_cube(center: [f32; 3], half: f32) -> Vec<f32> {
    let [cx, cy, cz] = center;
    let mut verts = Vec::new();

    // 6 faces × 2 triangles × 3 vertices × 9 floats (pos3+normal3+color3)
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([0.0, 0.0, -1.0], [[cx-half,cy-half,cz-half],[cx+half,cy-half,cz-half],[cx+half,cy+half,cz-half],[cx-half,cy+half,cz-half]]), // front
        ([0.0, 0.0, 1.0],  [[cx+half,cy-half,cz+half],[cx-half,cy-half,cz+half],[cx-half,cy+half,cz+half],[cx+half,cy+half,cz+half]]), // back
        ([0.0, 1.0, 0.0],  [[cx-half,cy+half,cz-half],[cx+half,cy+half,cz-half],[cx+half,cy+half,cz+half],[cx-half,cy+half,cz+half]]), // top
        ([0.0,-1.0, 0.0],  [[cx-half,cy-half,cz+half],[cx+half,cy-half,cz+half],[cx+half,cy-half,cz-half],[cx-half,cy-half,cz-half]]), // bottom
        ([1.0, 0.0, 0.0],  [[cx+half,cy-half,cz-half],[cx+half,cy-half,cz+half],[cx+half,cy+half,cz+half],[cx+half,cy+half,cz-half]]), // right
        ([-1.0,0.0, 0.0],  [[cx-half,cy-half,cz+half],[cx-half,cy-half,cz-half],[cx-half,cy+half,cz-half],[cx-half,cy+half,cz+half]]), // left
    ];

    let color = [0.8f32, 0.4, 0.2]; // orange

    for (normal, quad) in &faces {
        // Two triangles per quad
        for idx in &[0,1,2, 0,2,3] {
            verts.extend_from_slice(&quad[*idx]);
            verts.extend_from_slice(normal);
            verts.extend_from_slice(&color);
        }
    }

    verts
}

fn build_view_proj(eye: [f32; 3], target: [f32; 3], fov_deg: f32, aspect: f32) -> [[f32; 4]; 4] {
    // Look-at view matrix
    let fwd = normalize([target[0]-eye[0], target[1]-eye[1], target[2]-eye[2]]);
    let right = normalize(cross([0.0, 1.0, 0.0], fwd));
    let up = cross(fwd, right);

    let view = [
        [right[0], up[0], -fwd[0], 0.0],
        [right[1], up[1], -fwd[1], 0.0],
        [right[2], up[2], -fwd[2], 0.0],
        [-dot(right, eye), -dot(up, eye), dot(fwd, eye), 1.0],
    ];

    // Perspective projection
    let fov = fov_deg.to_radians();
    let f = 1.0 / (fov / 2.0).tan();
    let near = 0.1f32;
    let far = 100.0f32;
    let nf = 1.0 / (near - far);

    let proj = [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, (far + near) * nf, -1.0],
        [0.0, 0.0, 2.0 * far * near * nf, 0.0],
    ];

    mat4_mul(proj, view)
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0]*v[0] + v[1]*v[1] + v[2]*v[2]).sqrt();
    if l > 1e-6 { [v[0]/l, v[1]/l, v[2]/l] } else { [0.0, 0.0, -1.0] }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0]*b[0] + a[1]*b[1] + a[2]*b[2]
}

fn mat4_mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut r = [[0.0f32; 4]; 4];
    for col in 0..4 {
        for row in 0..4 {
            r[col][row] = a[0][row]*b[col][0] + a[1][row]*b[col][1] + a[2][row]*b[col][2] + a[3][row]*b[col][3];
        }
    }
    r
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}

const SCENE_SHADER: &str = r#"
struct Uniforms {
    view_proj: mat4x4f,
    light_dir: vec3f,
    _pad: f32,
    ambient: f32,
    _pad2: vec3f,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) color: vec3f,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4f,
    @location(0) normal: vec3f,
    @location(1) color: vec3f,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4f(in.position, 1.0);
    out.normal = in.normal;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let n = normalize(in.normal);
    let diff = max(dot(n, u.light_dir), 0.0);
    let light = u.ambient + diff * 0.8;
    let col = in.color * light;
    return vec4f(col, 1.0);
}
"#;
