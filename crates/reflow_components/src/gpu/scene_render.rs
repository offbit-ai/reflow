//! Scene Render Actor — rasterizes mesh-based scenes via wgpu.
//!
//! Takes a scene graph (objects with transforms) and actual mesh bytes
//! from upstream actors. Renders all triangles with per-object transforms,
//! basic diffuse lighting, and outputs RGBA bytes.
//!
//! Mesh formats accepted:
//! - `meshes` port: MarchingCubes format — 24-byte stride (pos3 + normal3), triangle list
//! - `terrain_mesh` port: HeightmapToMesh format — 32-byte stride (pos3 + normal3 + uv2),
//!    with u32 indices appended after vertex data

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

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

#[actor(
    SceneRenderActor,
    inports::<10>(scene, meshes, terrain_mesh, texture),
    outports::<1>(output, metadata, error),
    state(MemoryState)
)]
pub async fn scene_render_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
    let height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
    let fov = config.get("fov").and_then(|v| v.as_f64()).unwrap_or(45.0) as f32;
    let cam_pos = [
        config
            .get("cameraPosX")
            .and_then(|v| v.as_f64())
            .unwrap_or(8.0) as f32,
        config
            .get("cameraPosY")
            .and_then(|v| v.as_f64())
            .unwrap_or(6.0) as f32,
        config
            .get("cameraPosZ")
            .and_then(|v| v.as_f64())
            .unwrap_or(10.0) as f32,
    ];
    let cam_target = [
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
    ];
    let msaa_samples = config.get("msaa").and_then(|v| v.as_u64()).unwrap_or(4) as u32;
    let near_plane = config.get("near").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let far_plane = config.get("far").and_then(|v| v.as_f64()).unwrap_or(10000.0) as f32;
    let clear_color = [
        config.get("bgR").and_then(|v| v.as_f64()).unwrap_or(0.1),
        config.get("bgG").and_then(|v| v.as_f64()).unwrap_or(0.1),
        config.get("bgB").and_then(|v| v.as_f64()).unwrap_or(0.15),
    ];

    // Cache meshes and texture in state
    if let Some(Message::Bytes(b)) = payload.get("meshes") {
        use base64::Engine;
        ctx.pool_upsert(
            "_cache",
            "meshes_b64",
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(&**b)),
        );
    }
    if let Some(Message::Bytes(b)) = payload.get("terrain_mesh") {
        use base64::Engine;
        ctx.pool_upsert(
            "_cache",
            "terrain_b64",
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(&**b)),
        );
    }
    if let Some(Message::Bytes(b)) = payload.get("texture") {
        // Decode image once into static cache (zero-copy on subsequent frames)
        if get_texture_cache().lock().is_none() {
            if let Ok(img) = image::load_from_memory(&b) {
                let rgba = img.to_rgba8();
                let w = rgba.width();
                let h = rgba.height();
                let mut buf = Vec::with_capacity(8 + (w * h * 4) as usize);
                buf.extend_from_slice(&w.to_le_bytes());
                buf.extend_from_slice(&h.to_le_bytes());
                buf.extend_from_slice(rgba.as_raw());
                *get_texture_cache().lock() = Some(std::sync::Arc::new(buf));
            }
        }
    }

    // Scene is the per-frame trigger — if missing, just cache and return
    let scene_data = match payload.get("scene") {
        Some(Message::Object(obj)) => {
            let v: serde_json::Value = obj.as_ref().clone().into();
            v
        }
        _ => return Ok(HashMap::new()),
    };

    // Read cached meshes
    let cache: HashMap<String, serde_json::Value> = ctx.get_pool("_cache").into_iter().collect();
    let prefab_mesh: Option<Vec<u8>> = cache.get("meshes_b64").and_then(|v| v.as_str()).map(|s| {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .unwrap_or_default()
    });
    let terrain_mesh: Option<Vec<u8>> =
        cache.get("terrain_b64").and_then(|v| v.as_str()).map(|s| {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(s)
                .unwrap_or_default()
        });
    // Read pre-decoded RGBA texture from static cache (Arc clone = pointer bump, not 4MB copy)
    let texture_rgba: Option<std::sync::Arc<Vec<u8>>> = get_texture_cache().lock().clone();

    let objects = scene_data
        .get("objects")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let objects_clone = objects.clone();

    let pixels = tokio::task::spawn_blocking(move || {
        render_scene(
            width,
            height,
            fov,
            cam_pos,
            cam_target,
            msaa_samples,
            clear_color,
            near_plane,
            far_plane,
            &objects_clone,
            prefab_mesh.as_deref(),
            terrain_mesh.as_deref(),
            texture_rgba.as_deref().map(|v| v.as_slice()),
        )
    })
    .await
    .map_err(|e| anyhow::anyhow!("Spawn failed: {}", e))?
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    let mut results = HashMap::new();
    results.insert("output".to_string(), Message::bytes(pixels));
    results.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width,
            "height": height,
            "format": "RGBA8",
            "objectCount": objects.len(),
        }))),
    );
    Ok(results)
}

/// Parse MarchingCubes mesh: 24-byte stride (pos3 + normal3), triangle list.
/// Returns Vec of (position, normal) tuples.
fn parse_prefab_mesh(data: &[u8]) -> Vec<([f32; 3], [f32; 3])> {
    let stride = 24; // 6 floats × 4 bytes
    let vertex_count = data.len() / stride;
    let mut verts = Vec::with_capacity(vertex_count);

    for i in 0..vertex_count {
        let off = i * stride;
        if off + stride > data.len() {
            break;
        }
        let px = f32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        let py = f32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap());
        let pz = f32::from_le_bytes(data[off + 8..off + 12].try_into().unwrap());
        let nx = f32::from_le_bytes(data[off + 12..off + 16].try_into().unwrap());
        let ny = f32::from_le_bytes(data[off + 16..off + 20].try_into().unwrap());
        let nz = f32::from_le_bytes(data[off + 20..off + 24].try_into().unwrap());
        verts.push(([px, py, pz], [nx, ny, nz]));
    }
    verts
}

/// Parse HeightmapToMesh: 32-byte stride (pos3 + normal3 + uv2), u32 indices appended.
/// Returns (vertices, indices) where vertices are (position, normal).
fn parse_terrain_mesh(data: &[u8]) -> (Vec<([f32; 3], [f32; 3])>, Vec<u32>) {
    let stride = 32; // 8 floats × 4 bytes

    // We need to figure out where vertices end and indices begin.
    // HeightmapToMesh generates a grid: for WxH heightmap, vertices = W*H,
    // indices = (W-1)*(H-1)*6. Total = vertices*32 + indices*4.
    // We can detect the boundary: vertex data is always a multiple of 32,
    // and the total must satisfy: data.len() = vertex_bytes + index_bytes.
    // Try to find a split where vertex_bytes % 32 == 0 and index_bytes % 4 == 0
    // and index_count matches expected triangle count.

    // Heuristic: try common grid sizes. For a 64x64 grid:
    // vertices = 64*64 = 4096, vertex_bytes = 4096*32 = 131072
    // indices = 63*63*6 = 23814, index_bytes = 23814*4 = 95256
    // total = 226328
    //
    // General approach: assume indices are at the end, each is u32.
    // vertex_count * 32 + index_count * 4 = data.len()
    // For a grid: index_count = (sqrt(vertex_count)-1)^2 * 6
    // Solve numerically by trying grid sizes.

    let total = data.len();

    // Try grid sizes from 2 to 512
    let mut vertex_bytes = total; // fallback: all vertices, no indices
    let mut index_bytes = 0;

    for grid_size in 2..=512 {
        let vc = grid_size * grid_size;
        let vb = vc * stride;
        let ic = (grid_size - 1) * (grid_size - 1) * 6;
        let ib = ic * 4;
        if vb + ib == total {
            vertex_bytes = vb;
            index_bytes = ib;
            break;
        }
    }

    let vertex_count = vertex_bytes / stride;
    let mut verts = Vec::with_capacity(vertex_count);

    for i in 0..vertex_count {
        let off = i * stride;
        if off + stride > vertex_bytes {
            break;
        }
        let px = f32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        let py = f32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap());
        let pz = f32::from_le_bytes(data[off + 8..off + 12].try_into().unwrap());
        let nx = f32::from_le_bytes(data[off + 12..off + 16].try_into().unwrap());
        let ny = f32::from_le_bytes(data[off + 16..off + 20].try_into().unwrap());
        let nz = f32::from_le_bytes(data[off + 20..off + 24].try_into().unwrap());
        // uv at off+24..off+32 — we skip UV for now
        verts.push(([px, py, pz], [nx, ny, nz]));
    }

    let index_count = index_bytes / 4;
    let mut indices = Vec::with_capacity(index_count);
    for i in 0..index_count {
        let off = vertex_bytes + i * 4;
        if off + 4 > total {
            break;
        }
        let idx = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        indices.push(idx);
    }

    (verts, indices)
}

/// Apply translate + scale to a position.
fn transform_pos(pos: [f32; 3], translate: [f32; 3], scale: [f32; 3]) -> [f32; 3] {
    [
        pos[0] * scale[0] + translate[0],
        pos[1] * scale[1] + translate[1],
        pos[2] * scale[2] + translate[2],
    ]
}

/// Build render vertex buffer (pos3+normal3+color3 = 36 bytes/vertex) from actual mesh data.
fn build_vertex_buffer(
    objects: &[serde_json::Value],
    prefab_mesh: Option<&[u8]>,
    terrain_mesh_data: Option<&[u8]>,
) -> Vec<f32> {
    let mut all_vertices: Vec<f32> = Vec::new();

    // Determine mesh stride from scene objects (36 = pre-colored, 24 = standard)
    let mesh_stride = objects
        .first()
        .and_then(|o| o.get("meshStride"))
        .and_then(|v| v.as_u64())
        .unwrap_or(24) as usize;
    let prefab_is_colored = mesh_stride >= 36;
    let prefab_verts = if prefab_is_colored {
        None
    } else {
        prefab_mesh.as_ref().map(|m| parse_prefab_mesh(m))
    };
    let terrain_parsed = terrain_mesh_data.map(parse_terrain_mesh);

    // Fallback colors for objects without material
    let fallback_colors: [[f32; 3]; 6] = [
        [0.85, 0.45, 0.20], // orange
        [0.30, 0.55, 0.85], // blue
        [0.80, 0.25, 0.35], // red
        [0.60, 0.75, 0.30], // lime
        [0.70, 0.40, 0.75], // purple
        [0.90, 0.75, 0.25], // gold
    ];
    let mut color_idx = 0;

    for obj in objects {
        let transform = obj.get("transform").cloned().unwrap_or(json!({}));
        let pos = transform
            .get("position")
            .and_then(|p| p.as_array())
            .map(|a| {
                [
                    a.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                ]
            })
            .unwrap_or([0.0; 3]);

        let scl = transform
            .get("scale")
            .and_then(|s| s.as_array())
            .map(|a| {
                [
                    a.first().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                    a.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                    a.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                ]
            })
            .unwrap_or([1.0; 3]);

        let obj_type = obj
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("instance");

        match obj_type {
            "terrain" => {
                if let Some((ref verts, ref indices)) = terrain_parsed {
                    if !indices.is_empty() {
                        // Indexed mesh — expand indices to triangle list
                        for idx in indices {
                            let i = *idx as usize;
                            if i < verts.len() {
                                let (p, n) = verts[i];
                                let tp = transform_pos(p, pos, scl);
                                all_vertices.extend_from_slice(&tp);
                                all_vertices.extend_from_slice(&n);
                                // Color terrain based on height
                                let height_factor = (tp[1] + 1.0).max(0.0).min(3.0) / 3.0;
                                let c = [
                                    0.25 + 0.15 * height_factor,
                                    0.55 + 0.20 * height_factor,
                                    0.15 + 0.10 * (1.0 - height_factor),
                                ];
                                all_vertices.extend_from_slice(&c);
                            }
                        }
                    } else {
                        // Non-indexed — direct triangle list
                        for (p, n) in verts {
                            let tp = transform_pos(*p, pos, scl);
                            all_vertices.extend_from_slice(&tp);
                            all_vertices.extend_from_slice(n);
                            let height_factor = (tp[1] + 1.0).max(0.0).min(3.0) / 3.0;
                            let c = [
                                0.25 + 0.15 * height_factor,
                                0.55 + 0.20 * height_factor,
                                0.15 + 0.10 * (1.0 - height_factor),
                            ];
                            all_vertices.extend_from_slice(&c);
                        }
                    }
                } else {
                    // Fallback: flat grid placeholder
                    let tw = obj
                        .get("terrain")
                        .and_then(|t| t.get("width"))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(10.0) as f32;
                    let td = obj
                        .get("terrain")
                        .and_then(|t| t.get("depth"))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(10.0) as f32;
                    let segs = 8;
                    for gz in 0..segs {
                        for gx in 0..segs {
                            let x0 = pos[0] + (gx as f32 / segs as f32 - 0.5) * tw;
                            let z0 = pos[2] + (gz as f32 / segs as f32 - 0.5) * td;
                            let x1 = pos[0] + ((gx + 1) as f32 / segs as f32 - 0.5) * tw;
                            let z1 = pos[2] + ((gz + 1) as f32 / segs as f32 - 0.5) * td;
                            let y = pos[1];
                            let n = [0.0f32, 1.0, 0.0];
                            let c = [0.3f32, 0.6, 0.2];
                            for v in &[
                                [x0, y, z0],
                                [x1, y, z0],
                                [x1, y, z1],
                                [x0, y, z0],
                                [x1, y, z1],
                                [x0, y, z1],
                            ] {
                                all_vertices.extend_from_slice(v);
                                all_vertices.extend_from_slice(&n);
                                all_vertices.extend_from_slice(&c);
                            }
                        }
                    }
                }
            }
            _ => {
                // Instance — read material from scene object
                let mat = obj.get("material");
                let base_color = mat
                    .and_then(|m| m.get("color"))
                    .and_then(|c| c.as_array())
                    .map(|a| {
                        [
                            a.first().and_then(|v| v.as_f64()).unwrap_or(0.8) as f32,
                            a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.8) as f32,
                            a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.8) as f32,
                        ]
                    })
                    .unwrap_or_else(|| {
                        let c = fallback_colors[color_idx % fallback_colors.len()];
                        color_idx += 1;
                        c
                    });

                if prefab_is_colored {
                    // Pre-colored mesh (36-byte stride) — parse and pass through
                    if let Some(ref mesh) = prefab_mesh {
                        let stride = 36; // pos3+normal3+color3
                        let vc = mesh.len() / stride;
                        for vi in 0..vc {
                            let off = vi * stride;
                            if off + stride > mesh.len() {
                                break;
                            }
                            let px = f32::from_le_bytes(mesh[off..off + 4].try_into().unwrap());
                            let py = f32::from_le_bytes(mesh[off + 4..off + 8].try_into().unwrap());
                            let pz =
                                f32::from_le_bytes(mesh[off + 8..off + 12].try_into().unwrap());
                            let tp = transform_pos([px, py, pz], pos, scl);
                            all_vertices.extend_from_slice(&tp);
                            // normal (3 floats)
                            for k in 0..3 {
                                let f = f32::from_le_bytes(
                                    mesh[off + 12 + k * 4..off + 16 + k * 4].try_into().unwrap(),
                                );
                                all_vertices.push(f);
                            }
                            // color (3 floats) — from the mesh, not material
                            for k in 0..3 {
                                let f = f32::from_le_bytes(
                                    mesh[off + 24 + k * 4..off + 28 + k * 4].try_into().unwrap(),
                                );
                                all_vertices.push(f);
                            }
                        }
                    }
                } else if let Some(ref verts) = prefab_verts {
                    for (p, n) in verts {
                        let tp = transform_pos(*p, pos, scl);
                        all_vertices.extend_from_slice(&tp);
                        all_vertices.extend_from_slice(n);
                        all_vertices.extend_from_slice(&base_color);
                    }
                } else {
                    // Fallback: simple cube
                    let s = 0.4 * scl[0];
                    let cube = generate_cube(pos, s, base_color);
                    all_vertices.extend_from_slice(&cube);
                }
            }
        }
    }

    all_vertices
}

/// Cached render pipeline + bind group layout. Created once, reused every frame.
#[allow(dead_code)]
struct CachedScenePipeline {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    sample_count: u32,
}

/// Static texture cache — decoded RGBA bytes, bypasses JSON pool entirely.
/// Format: [w:u32 LE, h:u32 LE, rgba_pixels...]
/// Uses Arc to avoid cloning 4MB per frame.
static TEXTURE_CACHE: std::sync::OnceLock<parking_lot::Mutex<Option<std::sync::Arc<Vec<u8>>>>> = std::sync::OnceLock::new();

fn get_texture_cache() -> &'static parking_lot::Mutex<Option<std::sync::Arc<Vec<u8>>>> {
    TEXTURE_CACHE.get_or_init(|| parking_lot::Mutex::new(None))
}

/// Cached GPU diffuse texture + view + sampler. Uploaded once, reused every frame.
struct CachedGpuDiffuse {
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    _texture: wgpu::Texture, // prevent drop
}
static GPU_DIFFUSE_CACHE: std::sync::OnceLock<CachedGpuDiffuse> = std::sync::OnceLock::new();

use std::sync::OnceLock;
static SCENE_PIPELINE_4X: OnceLock<CachedScenePipeline> = OnceLock::new();
static SCENE_PIPELINE_1X: OnceLock<CachedScenePipeline> = OnceLock::new();

fn get_or_create_pipeline(
    device: &wgpu::Device,
    sample_count: u32,
) -> &'static CachedScenePipeline {
    let lock = if sample_count > 1 {
        &SCENE_PIPELINE_4X
    } else {
        &SCENE_PIPELINE_1X
    };
    lock.get_or_init(|| {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Scene Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SCENE_SHADER)),
        });
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
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[&bgl],
                    push_constant_ranges: &[],
                }),
            ),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 36,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 12,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 24,
                            shader_location: 2,
                        },
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
                cull_mode: None,
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
        CachedScenePipeline {
            pipeline,
            bgl,
            sample_count,
        }
    })
}

// ─── Textured pipeline (32-byte stride: pos3+normal3+uv2, with diffuse texture) ───

static TEXTURED_PIPELINE_1X: OnceLock<CachedScenePipeline> = OnceLock::new();
static TEXTURED_PIPELINE_4X: OnceLock<CachedScenePipeline> = OnceLock::new();

fn get_or_create_textured_pipeline(
    device: &wgpu::Device,
    sample_count: u32,
) -> &'static CachedScenePipeline {
    let lock = if sample_count > 1 { &TEXTURED_PIPELINE_4X } else { &TEXTURED_PIPELINE_1X };
    lock.get_or_init(|| {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Textured Scene Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(TEXTURED_SHADER)),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Textured Scene Pipeline"),
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
                    array_stride: 32, // pos3(12) + normal3(12) + uv2(8)
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },  // position
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 12, shader_location: 1 }, // normal
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 24, shader_location: 2 }, // uv
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
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState { count: sample_count, mask: !0, alpha_to_coverage_enabled: false },
            multiview: None,
            cache: None,
        });
        CachedScenePipeline { pipeline, bgl, sample_count }
    })
}

const TEXTURED_SHADER: &str = r#"
struct Uniforms {
    view_proj: mat4x4f,
    light_dir: vec3f,
    _pad: f32,
    ambient: f32,
    _pad2: vec3f,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var diffuse_tex: texture_2d<f32>;
@group(0) @binding(2) var diffuse_samp: sampler;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4f,
    @location(0) normal: vec3f,
    @location(1) uv: vec2f,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4f(in.position, 1.0);
    out.normal = in.normal;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let uv = vec2f(in.uv.x, 1.0 - in.uv.y); // flip V for OpenGL→Vulkan
    let tex_color = textureSample(diffuse_tex, diffuse_samp, uv);
    let n = normalize(in.normal);
    let diff = max(dot(n, u.light_dir), 0.0);
    let light = u.ambient + diff * 0.8;
    let col = tex_color.rgb * light;
    return vec4f(col, 1.0);
}
"#;

fn render_scene(
    width: u32,
    height: u32,
    fov: f32,
    cam_pos: [f32; 3],
    cam_target: [f32; 3],
    msaa_samples: u32,
    clear_color: [f64; 3],
    near: f32,
    far: f32,
    objects: &[serde_json::Value],
    prefab_mesh: Option<&[u8]>,
    terrain_mesh: Option<&[u8]>,
    texture_rgba: Option<&[u8]>, // [w:u32, h:u32, rgba pixels...] or None
) -> Result<Vec<u8>, String> {
    use wgpu::util::DeviceExt;

    let ctx = &*crate::gpu::context::GPU_CONTEXT;
    let device = ctx.device();
    let queue = ctx.queue();

    // Check if we have a textured mesh (32-byte stride: pos3+normal3+uv2)
    let mesh_stride = objects
        .first()
        .and_then(|o| o.get("meshStride"))
        .and_then(|v| v.as_u64())
        .unwrap_or(24) as usize;
    let has_texture = texture_rgba.is_some() && mesh_stride == 32 && prefab_mesh.is_some();

    let (vertex_data, vertex_count, use_textured_pipeline) = if has_texture {
        // Textured path: pass mesh data as-is (32-byte stride: pos3+normal3+uv2)
        let mesh = prefab_mesh.unwrap();
        let vc = mesh.len() / 32;
        // Apply per-object transform to vertices
        let transform = objects.first()
            .and_then(|o| o.get("transform"))
            .cloned()
            .unwrap_or(serde_json::json!({}));
        let pos = transform.get("position").and_then(|p| p.as_array())
            .map(|a| [
                a.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            ]).unwrap_or([0.0; 3]);
        let scl = transform.get("scale").and_then(|s| s.as_array())
            .map(|a| [
                a.first().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
            ]).unwrap_or([1.0; 3]);

        let mut transformed = Vec::with_capacity(mesh.len());
        for i in 0..vc {
            let off = i * 32;
            let px = f32::from_le_bytes(mesh[off..off+4].try_into().unwrap());
            let py = f32::from_le_bytes(mesh[off+4..off+8].try_into().unwrap());
            let pz = f32::from_le_bytes(mesh[off+8..off+12].try_into().unwrap());
            let tp = transform_pos([px, py, pz], pos, scl);
            transformed.extend_from_slice(&tp[0].to_le_bytes());
            transformed.extend_from_slice(&tp[1].to_le_bytes());
            transformed.extend_from_slice(&tp[2].to_le_bytes());
            // Copy normal (12 bytes) + UV (8 bytes) as-is
            transformed.extend_from_slice(&mesh[off+12..off+32]);
        }
        (transformed, vc, true)
    } else {
        // Standard path: build colored vertex buffer (36-byte stride)
        let all_vertices = build_vertex_buffer(objects, prefab_mesh, terrain_mesh);
        if all_vertices.is_empty() {
            return Ok(vec![30; (width * height * 4) as usize]);
        }
        let vc = all_vertices.len() / 9;
        (bytemuck::cast_slice(&all_vertices).to_vec(), vc, false)
    };

    if vertex_count == 0 {
        return Ok(vec![30; (width * height * 4) as usize]);
    }

    let sample_count = match msaa_samples {
        1 => 1,
        2 => 2,
        _ => 4,
    };

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Vertices"),
        contents: &vertex_data,
        usage: wgpu::BufferUsages::VERTEX,
    });

    let view_proj = build_view_proj(cam_pos, cam_target, fov, width as f32 / height as f32, near, far);
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

    // Textures (per-frame — size may differ between calls)
    let resolve_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Resolve"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let resolve_view = resolve_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let color_usage = if sample_count > 1 {
        wgpu::TextureUsages::RENDER_ATTACHMENT
    } else {
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC
    };
    let color_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Color"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: color_usage,
        view_formats: &[],
    });
    let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    if use_textured_pipeline {
        // ─── Textured rendering path ───
        let cached_pipe = get_or_create_textured_pipeline(device, sample_count);

        // Upload GPU texture once (OnceLock), reuse view+sampler every frame
        let gpu_diff = GPU_DIFFUSE_CACHE.get_or_init(|| {
            let tex_data = texture_rgba.as_ref().unwrap();
            let tex_w = u32::from_le_bytes(tex_data[0..4].try_into().unwrap());
            let tex_h = u32::from_le_bytes(tex_data[4..8].try_into().unwrap());
            let tex_pixels = &tex_data[8..];
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Diffuse"),
                size: wgpu::Extent3d { width: tex_w, height: tex_h, depth_or_array_layers: 1 },
                mip_level_count: 1, sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                tex_pixels,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(tex_w * 4), rows_per_image: Some(tex_h) },
                wgpu::Extent3d { width: tex_w, height: tex_h, depth_or_array_layers: 1 },
            );
            CachedGpuDiffuse {
                view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
                sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                }),
                _texture: texture,
            }
        });

        // Bind group per frame (cheap — just references cached texture + new uniform)
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &cached_pipe.bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&gpu_diff.view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&gpu_diff.sampler) },
            ],
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Textured Scene Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: if sample_count > 1 { Some(&resolve_view) } else { None },
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: clear_color[0], g: clear_color[1], b: clear_color[2], a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&cached_pipe.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..vertex_count as u32, 0..1);
        }
    } else {
        // ─── Standard vertex-color rendering path ───
        let cached = get_or_create_pipeline(device, sample_count);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &cached.bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scene Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: if sample_count > 1 { Some(&resolve_view) } else { None },
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: clear_color[0], g: clear_color[1], b: clear_color[2], a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&cached.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..vertex_count as u32, 0..1);
        }
    }

    // Readback
    let padded_row = (width * 4 + 255) & !255;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Readback"),
        size: (padded_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let readback_src = if sample_count > 1 {
        &resolve_texture
    } else {
        &color_texture
    };
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
    readback.unmap();

    Ok(pixels)
}

fn generate_cube(center: [f32; 3], half: f32, color: [f32; 3]) -> Vec<f32> {
    let [cx, cy, cz] = center;
    let mut verts = Vec::new();

    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        (
            [0.0, 0.0, -1.0],
            [
                [cx - half, cy - half, cz - half],
                [cx + half, cy - half, cz - half],
                [cx + half, cy + half, cz - half],
                [cx - half, cy + half, cz - half],
            ],
        ),
        (
            [0.0, 0.0, 1.0],
            [
                [cx + half, cy - half, cz + half],
                [cx - half, cy - half, cz + half],
                [cx - half, cy + half, cz + half],
                [cx + half, cy + half, cz + half],
            ],
        ),
        (
            [0.0, 1.0, 0.0],
            [
                [cx - half, cy + half, cz - half],
                [cx + half, cy + half, cz - half],
                [cx + half, cy + half, cz + half],
                [cx - half, cy + half, cz + half],
            ],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                [cx - half, cy - half, cz + half],
                [cx + half, cy - half, cz + half],
                [cx + half, cy - half, cz - half],
                [cx - half, cy - half, cz - half],
            ],
        ),
        (
            [1.0, 0.0, 0.0],
            [
                [cx + half, cy - half, cz - half],
                [cx + half, cy - half, cz + half],
                [cx + half, cy + half, cz + half],
                [cx + half, cy + half, cz - half],
            ],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                [cx - half, cy - half, cz + half],
                [cx - half, cy - half, cz - half],
                [cx - half, cy + half, cz - half],
                [cx - half, cy + half, cz + half],
            ],
        ),
    ];

    for (normal, quad) in &faces {
        for idx in &[0, 1, 2, 0, 2, 3] {
            verts.extend_from_slice(&quad[*idx]);
            verts.extend_from_slice(normal);
            verts.extend_from_slice(&color);
        }
    }

    verts
}

fn build_view_proj(eye: [f32; 3], target: [f32; 3], fov_deg: f32, aspect: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let fwd = normalize([target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]]);
    let right = normalize(cross([0.0, 1.0, 0.0], fwd));
    let up = cross(fwd, right);

    let view = [
        [right[0], up[0], -fwd[0], 0.0],
        [right[1], up[1], -fwd[1], 0.0],
        [right[2], up[2], -fwd[2], 0.0],
        [-dot(right, eye), -dot(up, eye), dot(fwd, eye), 1.0],
    ];

    let fov = fov_deg.to_radians();
    let f = 1.0 / (fov / 2.0).tan();
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
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 1e-6 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0, 0.0, -1.0]
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn mat4_mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut r = [[0.0f32; 4]; 4];
    for col in 0..4 {
        for row in 0..4 {
            r[col][row] = a[0][row] * b[col][0]
                + a[1][row] * b[col][1]
                + a[2][row] * b[col][2]
                + a[3][row] * b[col][3];
        }
    }
    r
}

#[allow(dead_code)]
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
    @location(2) world_pos: vec3f,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4f(in.position, 1.0);
    out.normal = in.normal;
    out.color = in.color;
    out.world_pos = in.position;
    return out;
}

fn procedural_pattern(p: vec3f, base: vec3f) -> vec3f {
    let u_coord = p.x * 1.5;
    let v_coord = atan2(p.y, max(abs(p.z), 0.001)) * 3.0;
    let diamond = sin(u_coord * 8.0 + v_coord) * sin(u_coord * 8.0 - v_coord);
    let scales = smoothstep(-0.1, 0.1, diamond);
    let stripe = smoothstep(0.3, 0.7, sin(u_coord * 3.0) * 0.5 + 0.5);
    let dorsal = smoothstep(0.85, 0.95, cos(v_coord / 3.0));
    let pattern = scales * 0.4 + stripe * 0.3 + 0.3;
    let dark = base * 0.5;
    var col = mix(dark, base, pattern);
    col = mix(col, base * 0.3, dorsal * 0.5);
    return col;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let n = normalize(in.normal);
    let diff = max(dot(n, u.light_dir), 0.0);
    let light = u.ambient + diff * 0.8;
    let patterned = procedural_pattern(in.world_pos, in.color);
    let col = patterned * light;
    return vec4f(col, 1.0);
}
"#;
