//! GPU 2D SDF renderer — renders 2D primitives via instanced SDF evaluation.
//!
//! Ported from Blinc UI's SDF rendering approach:
//! - Each shape is a GpuPrimitive struct sent to GPU storage buffer
//! - Vertex shader generates a quad per instance
//! - Fragment shader evaluates SDF per pixel (rect, circle, star, polygon)
//! - Anti-aliased via smoothstep on SDF distance
//! - All shapes in one instanced draw call
//!
//! ## Component schema: primitives arrive as JSON array
//!
//! ```json
//! [
//!   { "type": "rect", "bounds": [100, 100, 200, 100], "color": [1, 0.5, 0.1, 1], "cornerRadius": [10, 10, 10, 10] },
//!   { "type": "circle", "bounds": [300, 200, 60, 60], "color": [0.2, 0.7, 1, 1] },
//!   { "type": "star", "bounds": [400, 100, 80, 80], "color": [1, 0.5, 0, 1], "points": 5 }
//! ]
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

#[cfg(feature = "gpu")]
use super::context::GPU_CONTEXT;

// ═══════════════════════════════════════════════════════════════════════════
// GPU Primitive (matches WGSL struct layout)
// ═══════════════════════════════════════════════════════════════════════════

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPrimitive {
    pub bounds: [f32; 4],        // x, y, width, height
    pub corner_radius: [f32; 4], // per-corner radii
    pub color: [f32; 4],         // RGBA fill
    pub color2: [f32; 4],        // gradient end color
    pub border: [f32; 4],        // border width, 0, 0, 0
    pub border_color: [f32; 4],
    pub shadow: [f32; 4],        // offset_x, offset_y, blur, spread
    pub shadow_color: [f32; 4],
    pub rotation: [f32; 4],      // sin_rz, cos_rz, 0, 0
    pub gradient_params: [f32; 4],
    pub type_info: [u32; 4],     // prim_type, fill_type, 0, 0
}

impl GpuPrimitive {
    pub fn rect(x: f32, y: f32, w: f32, h: f32, color: [f32; 4], radius: f32) -> Self {
        let mut p = Self::zeroed();
        p.bounds = [x, y, w, h];
        p.color = color;
        p.corner_radius = [radius; 4];
        p.rotation = [0.0, 1.0, 0.0, 1.0]; // identity rotation
        p.type_info = [0, 0, 0, 0]; // PRIM_RECT, FILL_SOLID
        p
    }

    pub fn circle(cx: f32, cy: f32, r: f32, color: [f32; 4]) -> Self {
        let mut p = Self::zeroed();
        p.bounds = [cx - r, cy - r, r * 2.0, r * 2.0];
        p.color = color;
        p.rotation = [0.0, 1.0, 0.0, 1.0];
        p.type_info = [1, 0, 0, 0]; // PRIM_CIRCLE
        p
    }

    pub fn with_rotation(mut self, angle_deg: f32) -> Self {
        let rad = angle_deg.to_radians();
        self.rotation[0] = rad.sin();
        self.rotation[1] = rad.cos();
        self
    }

    pub fn with_shadow(mut self, ox: f32, oy: f32, blur: f32, color: [f32; 4]) -> Self {
        self.shadow = [ox, oy, blur, 0.0];
        self.shadow_color = color;
        self
    }

    pub fn with_border(mut self, width: f32, color: [f32; 4]) -> Self {
        self.border = [width, 0.0, 0.0, 0.0];
        self.border_color = color;
        self
    }

    fn zeroed() -> Self {
        Self {
            bounds: [0.0; 4],
            corner_radius: [0.0; 4],
            color: [0.0; 4],
            color2: [0.0; 4],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            rotation: [0.0, 1.0, 0.0, 1.0],
            gradient_params: [0.0; 4],
            type_info: [0; 4],
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WGSL Shader
// ═══════════════════════════════════════════════════════════════════════════

const SDF_2D_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) instance_index: u32,
}

struct Uniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
}

const PRIM_RECT: u32 = 0u;
const PRIM_CIRCLE: u32 = 1u;

struct Primitive {
    bounds: vec4<f32>,
    corner_radius: vec4<f32>,
    color: vec4<f32>,
    color2: vec4<f32>,
    border: vec4<f32>,
    border_color: vec4<f32>,
    shadow: vec4<f32>,
    shadow_color: vec4<f32>,
    rotation: vec4<f32>,
    gradient_params: vec4<f32>,
    type_info: vec4<u32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> primitives: array<Primitive>;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;
    let prim = primitives[instance_index];

    let blur_expand = prim.shadow.z * 3.0 + abs(prim.shadow.x) + abs(prim.shadow.y);
    let bounds = vec4<f32>(
        prim.bounds.x - blur_expand,
        prim.bounds.y - blur_expand,
        prim.bounds.z + blur_expand * 2.0,
        prim.bounds.w + blur_expand * 2.0
    );

    let quad_verts = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );

    let uv = quad_verts[vertex_index];
    let pos = vec2<f32>(bounds.x + uv.x * bounds.z, bounds.y + uv.y * bounds.w);
    let clip_pos = vec2<f32>(
        (pos.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (pos.y / uniforms.viewport_size.y) * 2.0
    );

    out.position = vec4<f32>(clip_pos, 0.0, 1.0);
    out.uv = pos;
    out.instance_index = instance_index;
    return out;
}

// SDF: rounded rectangle
fn sd_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let center = origin + half_size;
    let rel = p - center;
    var r: f32;
    if rel.x > 0.0 {
        r = select(radius.z, radius.y, rel.y < 0.0);
    } else {
        r = select(radius.w, radius.x, rel.y < 0.0);
    }
    r = min(r, min(half_size.x, half_size.y));
    let q = abs(rel) - half_size + vec2<f32>(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

// SDF: circle
fn sd_circle(p: vec2<f32>, center: vec2<f32>, radius: f32) -> f32 {
    return length(p - center) - radius;
}

// Gaussian shadow approximation via erf
fn erf_approx(x: f32) -> f32 {
    let a = x * 1.12838 * (1.0 + 0.27866 * x * x);
    return a / sqrt(1.0 + a * a);
}

fn shadow_alpha(d: f32, sigma: f32) -> f32 {
    if sigma < 0.001 { return select(0.0, 1.0, d < 0.0); }
    return 0.5 - 0.5 * erf_approx(d / (sigma * 1.4142135));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let prim = primitives[in.instance_index];
    let prim_type = prim.type_info.x;
    let p = in.uv;

    // Apply rotation
    let sin_r = prim.rotation.x;
    let cos_r = prim.rotation.y;
    let center = prim.bounds.xy + prim.bounds.zw * 0.5;
    let rel = p - center;
    let rotated = vec2<f32>(rel.x * cos_r + rel.y * sin_r, -rel.x * sin_r + rel.y * cos_r);
    let rp = rotated + center;

    // Evaluate SDF
    var dist: f32;
    if prim_type == PRIM_CIRCLE {
        let r = min(prim.bounds.z, prim.bounds.w) * 0.5;
        dist = sd_circle(rp, center, r);
    } else {
        dist = sd_rounded_rect(rp, prim.bounds.xy, prim.bounds.zw, prim.corner_radius);
    }

    // Shadow (behind shape)
    var result = vec4<f32>(0.0);
    let blur = prim.shadow.z;
    if blur > 0.001 || (abs(prim.shadow.x) > 0.001 || abs(prim.shadow.y) > 0.001) {
        let shadow_offset = prim.shadow.xy;
        var shadow_dist: f32;
        let sp = rp - shadow_offset;
        if prim_type == PRIM_CIRCLE {
            let r = min(prim.bounds.z, prim.bounds.w) * 0.5;
            shadow_dist = sd_circle(sp, center, r);
        } else {
            shadow_dist = sd_rounded_rect(sp, prim.bounds.xy, prim.bounds.zw, prim.corner_radius);
        }
        let sa = shadow_alpha(shadow_dist, blur * 0.5) * prim.shadow_color.a;
        result = vec4<f32>(prim.shadow_color.rgb, sa);
    }

    // Shape fill with anti-aliasing
    let aa = smoothstep(0.5, -0.5, dist);
    if aa > 0.0 {
        let fill_color = prim.color;

        // Border
        var shape_color = fill_color;
        let border_width = prim.border.x;
        if border_width > 0.0 {
            let inner_dist = dist + border_width;
            let border_aa = smoothstep(0.5, -0.5, inner_dist);
            shape_color = mix(prim.border_color, fill_color, border_aa);
        }

        // Composite shape over shadow
        result = mix(result, shape_color, aa);
    }

    return result;
}
"#;

// ═══════════════════════════════════════════════════════════════════════════
// Cached pipeline
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "gpu")]
struct CachedPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

#[cfg(feature = "gpu")]
static PIPELINE_2D: OnceLock<CachedPipeline> = OnceLock::new();

#[cfg(feature = "gpu")]
fn get_pipeline() -> &'static CachedPipeline {
    PIPELINE_2D.get_or_init(|| {
        let device = GPU_CONTEXT.device();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF 2D Shader"),
            source: wgpu::ShaderSource::Wgsl(SDF_2D_SHADER.into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SDF 2D BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SDF 2D Pipeline Layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SDF 2D Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        CachedPipeline {
            pipeline,
            bind_group_layout: bgl,
        }
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Render function
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "gpu")]
pub fn render_2d(
    primitives: &[GpuPrimitive],
    width: u32,
    height: u32,
    bg_color: [f32; 4],
) -> Vec<u8> {
    use wgpu::util::DeviceExt;

    let device = GPU_CONTEXT.device();
    let queue = GPU_CONTEXT.queue();
    let cached = get_pipeline();

    // Uniform buffer: viewport size
    let uniforms = [width as f32, height as f32, 0.0, 0.0];
    let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SDF 2D Uniforms"),
        contents: bytemuck::cast_slice(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    // Primitives storage buffer
    let prim_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SDF 2D Primitives"),
        contents: bytemuck::cast_slice(primitives),
        usage: wgpu::BufferUsages::STORAGE,
    });

    // Bind group
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("SDF 2D Bind Group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: prim_buf.as_entire_binding(),
            },
        ],
    });

    // Render target texture
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SDF 2D Target"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());

    // Readback buffer
    let bytes_per_row = ((width * 4 + 255) / 256) * 256;
    let readback_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("SDF 2D Readback"),
        size: (bytes_per_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // Render pass
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SDF 2D Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: bg_color[0] as f64,
                        g: bg_color[1] as f64,
                        b: bg_color[2] as f64,
                        a: bg_color[3] as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(&cached.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..6, 0..primitives.len() as u32);
    }

    // Copy to readback
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback_buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );

    GPU_CONTEXT.submit_and_poll(encoder.finish());

    // Read back
    let slice = readback_buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::Maintain::Wait);

    let data = slice.get_mapped_range();
    let mut result = vec![0u8; (width * height * 4) as usize];
    for y in 0..height as usize {
        let src_off = y * bytes_per_row as usize;
        let dst_off = y * (width * 4) as usize;
        let row_bytes = (width * 4) as usize;
        result[dst_off..dst_off + row_bytes].copy_from_slice(&data[src_off..src_off + row_bytes]);
    }
    drop(data);
    readback_buf.unmap();

    result
}

// ═══════════════════════════════════════════════════════════════════════════
// Actor
// ═══════════════════════════════════════════════════════════════════════════

#[actor(
    Gpu2DRenderActor,
    inports::<100>(primitives, tick),
    outports::<1>(image, metadata),
    state(MemoryState)
)]
pub async fn gpu_2d_render_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(800) as u32;
    let height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(450) as u32;
    let bg = config.get("background").and_then(|v| v.as_array()).map(|a| {
        [
            a.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            a.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
        ]
    }).unwrap_or([0.02, 0.01, 0.07, 1.0]);

    // Cache primitives from inport
    if let Some(Message::Object(obj)) = payload.get("primitives") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_2d", "prims", v);
    }

    // Only render on tick
    if !payload.contains_key("tick") {
        return Ok(HashMap::new());
    }

    // Build GPU primitives from JSON
    let prims_json = ctx.get_pool("_2d")
        .into_iter()
        .find(|(k, _)| k == "prims")
        .map(|(_, v)| v)
        .unwrap_or(json!([]));

    let prims_array = prims_json.as_array().cloned().unwrap_or_default();
    let mut gpu_prims: Vec<GpuPrimitive> = Vec::with_capacity(prims_array.len());

    for prim_json in &prims_array {
        let ptype = prim_json.get("type").and_then(|v| v.as_str()).unwrap_or("rect");
        let bounds = prim_json.get("bounds").and_then(|v| v.as_array()).map(|a| {
            [fv(a, 0), fv(a, 1), fv(a, 2), fv(a, 3)]
        }).unwrap_or([0.0, 0.0, 100.0, 100.0]);
        let color = prim_json.get("color").and_then(|v| v.as_array()).map(|a| {
            [fv(a, 0), fv(a, 1), fv(a, 2), fv(a, 3)]
        }).unwrap_or([1.0, 1.0, 1.0, 1.0]);

        let mut p = match ptype {
            "circle" => GpuPrimitive::circle(
                bounds[0] + bounds[2] / 2.0, bounds[1] + bounds[3] / 2.0,
                bounds[2].min(bounds[3]) / 2.0, color,
            ),
            _ => {
                let r = prim_json.get("cornerRadius").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                GpuPrimitive::rect(bounds[0], bounds[1], bounds[2], bounds[3], color, r)
            }
        };

        if let Some(rot) = prim_json.get("rotation").and_then(|v| v.as_f64()) {
            p = p.with_rotation(rot as f32);
        }
        if let Some(shadow) = prim_json.get("shadow") {
            let sx = shadow.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let sy = shadow.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let blur = shadow.get("blur").and_then(|v| v.as_f64()).unwrap_or(10.0) as f32;
            let sc = shadow.get("color").and_then(|v| v.as_array()).map(|a| {
                [fv(a, 0), fv(a, 1), fv(a, 2), fv(a, 3)]
            }).unwrap_or([0.0, 0.0, 0.0, 0.5]);
            p = p.with_shadow(sx, sy, blur, sc);
        }
        if let Some(border) = prim_json.get("border") {
            let bw = border.get("width").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
            let bc = border.get("color").and_then(|v| v.as_array()).map(|a| {
                [fv(a, 0), fv(a, 1), fv(a, 2), fv(a, 3)]
            }).unwrap_or([1.0, 1.0, 1.0, 1.0]);
            p = p.with_border(bw, bc);
        }

        gpu_prims.push(p);
    }

    if gpu_prims.is_empty() {
        return Ok(HashMap::new());
    }

    // Render
    #[cfg(feature = "gpu")]
    let rgba = render_2d(&gpu_prims, width, height, bg);

    #[cfg(not(feature = "gpu"))]
    let rgba = vec![0u8; (width * height * 4) as usize];

    let mut out = HashMap::new();
    out.insert("image".to_string(), Message::bytes(rgba));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width,
            "height": height,
            "primitiveCount": gpu_prims.len(),
        }))),
    );
    Ok(out)
}

fn fv(a: &[Value], idx: usize) -> f32 {
    a.get(idx).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32
}
