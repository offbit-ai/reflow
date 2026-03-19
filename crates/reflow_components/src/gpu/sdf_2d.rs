//! GPU 2D SDF renderer — renders 2D primitives via instanced SDF evaluation.
//!
//! Ported from Blinc UI's SDF rendering approach:
//! - Each shape is a GpuPrimitive struct sent to GPU storage buffer
//! - Vertex shader generates a quad per instance
//! - Fragment shader evaluates SDF per pixel (rect, circle, segment)
//! - Anti-aliased via smoothstep on SDF distance
//! - All shapes in one instanced draw call
//!
//! ## Animation convention
//!
//! The renderer accepts a single `values` inport from one AnimationTimeline.
//! Track names use prefixes to dispatch per-element:
//!
//! - `sN_x`, `sN_y`, `sN_scale`, `sN_rotation`, `sN_opacity` — shape N
//! - `cN_scale`, `cN_y`, `cN_opacity` — text character N
//!
//! ## Text rendering
//!
//! Config `text` array entries are rendered as SDF line segments using a
//! built-in geometric vector font. Each character's segments share the
//! same animation slot `cN_*`.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

#[cfg(feature = "gpu")]
use super::context::GPU_CONTEXT;
use super::font_atlas::GlyphAtlasGpu;

// ═══════════════════════════════════════════════════════════════════════════
// GPU Primitive (matches WGSL struct layout)
// ═══════════════════════════════════════════════════════════════════════════

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPrimitive {
    pub bounds: [f32; 4],        // x, y, width, height (AABB for segments)
    pub corner_radius: [f32; 4], // per-corner radii; [0]=thickness for segments
    pub color: [f32; 4],         // RGBA fill
    pub color2: [f32; 4],        // gradient end color
    pub border: [f32; 4],        // border width, 0, 0, 0
    pub border_color: [f32; 4],
    pub shadow: [f32; 4], // offset_x, offset_y, blur, spread
    pub shadow_color: [f32; 4],
    pub rotation: [f32; 4],        // sin_rz, cos_rz, 0, 0
    pub gradient_params: [f32; 4], // segment endpoints (x1, y1, x2, y2)
    pub type_info: [u32; 4],       // prim_type, fill_type, 0, 0
}

impl GpuPrimitive {
    pub fn rect(x: f32, y: f32, w: f32, h: f32, color: [f32; 4], radius: f32) -> Self {
        let mut p = Self::zeroed();
        p.bounds = [x, y, w, h];
        p.color = color;
        p.corner_radius = [radius; 4];
        p.rotation = [0.0, 1.0, 0.0, 1.0];
        p.type_info = [0, 0, 0, 0]; // PRIM_RECT
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

    pub fn segment(x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: [f32; 4]) -> Self {
        let pad = thickness + 2.0;
        let min_x = x1.min(x2) - pad;
        let min_y = y1.min(y2) - pad;
        let max_x = x1.max(x2) + pad;
        let max_y = y1.max(y2) + pad;
        let mut p = Self::zeroed();
        p.bounds = [min_x, min_y, max_x - min_x, max_y - min_y];
        p.gradient_params = [x1, y1, x2, y2];
        p.corner_radius = [thickness, 0.0, 0.0, 0.0];
        p.color = color;
        p.rotation = [0.0, 1.0, 0.0, 1.0];
        p.type_info = [2, 0, 0, 0]; // PRIM_SEGMENT
        p
    }

    /// Textured glyph quad — samples from SDF atlas texture.
    /// `uv` = [u0, v0, u1, v1] in atlas texture coordinates (0..1).
    pub fn glyph(x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], color: [f32; 4]) -> Self {
        let mut p = Self::zeroed();
        p.bounds = [x, y, w, h];
        p.gradient_params = uv; // atlas UV coordinates
        p.color = color;
        p.rotation = [0.0, 1.0, 0.0, 1.0];
        p.type_info = [3, 0, 0, 0]; // PRIM_GLYPH
        p
    }

    /// Mark this primitive as shadow-only (fill is skipped in the shader).
    /// Used to emit a shadow pre-pass primitive before the fill primitive.
    pub fn as_shadow_only(mut self) -> Self {
        self.type_info[1] |= 1;
        self
    }

    /// Zero out shadow params so this primitive renders fill only.
    pub fn clear_shadow(mut self) -> Self {
        self.shadow = [0.0; 4];
        self.shadow_color = [0.0; 4];
        self
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
const PRIM_SEGMENT: u32 = 2u;
const PRIM_GLYPH: u32 = 3u;

// type_info.y flags
const FLAG_SHADOW_ONLY: u32 = 1u; // skip fill; used for the shadow pre-pass

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

@group(1) @binding(0) var glyph_atlas: texture_2d<f32>;
@group(1) @binding(1) var glyph_sampler: sampler;

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

// SDF: line segment (rounded caps)
fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
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

fn eval_sdf(p: vec2<f32>, prim: Primitive) -> f32 {
    let prim_type = prim.type_info.x;
    if prim_type == PRIM_CIRCLE {
        let center = prim.bounds.xy + prim.bounds.zw * 0.5;
        let r = min(prim.bounds.z, prim.bounds.w) * 0.5;
        return sd_circle(p, center, r);
    } else if prim_type == PRIM_SEGMENT {
        let a = prim.gradient_params.xy;
        let b = prim.gradient_params.zw;
        let thickness = prim.corner_radius.x;
        return sd_segment(p, a, b) - thickness;
    } else {
        return sd_rounded_rect(p, prim.bounds.xy, prim.bounds.zw, prim.corner_radius);
    }
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let prim = primitives[in.instance_index];
    let prim_type = prim.type_info.x;
    let p = in.uv;

    // PRIM_GLYPH: sample SDF atlas texture with screen-space adaptive AA
    if prim_type == PRIM_GLYPH {
        let uv0 = prim.gradient_params.xy;
        let uv1 = prim.gradient_params.zw;
        let frac = (p - prim.bounds.xy) / prim.bounds.zw;
        let tex_uv = mix(uv0, uv1, clamp(frac, vec2<f32>(0.0), vec2<f32>(1.0)));
        let sdf_val = textureSample(glyph_atlas, glyph_sampler, tex_uv).r;
        // Screen-space derivative for pixel-perfect AA at any scale (Valve SDF technique)
        let fw = fwidth(sdf_val);
        let edge = clamp(fw * 0.7, 0.02, 0.25);
        let alpha = smoothstep(0.5 - edge, 0.5 + edge, sdf_val) * prim.color.a;
        if alpha < 0.001 { discard; }
        return vec4<f32>(prim.color.rgb, alpha);
    }

    // Apply rotation around bounds center
    let sin_r = prim.rotation.x;
    let cos_r = prim.rotation.y;
    let center = prim.bounds.xy + prim.bounds.zw * 0.5;
    let rel = p - center;
    let rotated = vec2<f32>(rel.x * cos_r + rel.y * sin_r, -rel.x * sin_r + rel.y * cos_r);
    let rp = rotated + center;

    let dist = eval_sdf(rp, prim);

    // Shadow (behind shape)
    var result = vec4<f32>(0.0);
    let blur = prim.shadow.z;
    if blur > 0.001 || (abs(prim.shadow.x) > 0.001 || abs(prim.shadow.y) > 0.001) {
        let sp = rp - prim.shadow.xy;
        let shadow_dist = eval_sdf(sp, prim);
        let sa = shadow_alpha(shadow_dist, blur * 0.5) * prim.shadow_color.a;
        result = vec4<f32>(prim.shadow_color.rgb, sa);
    }

    // Shadow-only primitive: skip fill and return early.
    // Used by the shadow pre-pass so every shadow renders before every fill.
    if (prim.type_info.y & FLAG_SHADOW_ONLY) != 0u {
        if result.a < 0.001 { discard; }
        return result;
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
    atlas_bind_group_layout: wgpu::BindGroupLayout,
    sample_count: u32,
}

#[cfg(feature = "gpu")]
static PIPELINE_2D_1X: OnceLock<CachedPipeline> = OnceLock::new();
#[cfg(feature = "gpu")]
static PIPELINE_2D_4X: OnceLock<CachedPipeline> = OnceLock::new();

#[cfg(feature = "gpu")]
fn get_pipeline(msaa: u32) -> &'static CachedPipeline {
    let (lock, sample_count) = if msaa > 1 {
        (&PIPELINE_2D_4X, 4u32)
    } else {
        (&PIPELINE_2D_1X, 1u32)
    };
    lock.get_or_init(|| {
        let device = GPU_CONTEXT.device();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF 2D Shader"),
            source: wgpu::ShaderSource::Wgsl(SDF_2D_SHADER.into()),
        });

        // Group 0: uniforms + primitives storage
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

        // Group 1: glyph atlas texture + sampler
        let atlas_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SDF 2D Atlas BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SDF 2D Pipeline Layout"),
            bind_group_layouts: &[&bgl, &atlas_bgl],
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
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        CachedPipeline {
            pipeline,
            bind_group_layout: bgl,
            atlas_bind_group_layout: atlas_bgl,
            sample_count,
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
    glyph_atlas: Option<&GlyphAtlasGpu>,
    msaa: u32,
) -> Vec<u8> {
    use wgpu::util::DeviceExt;

    let device = GPU_CONTEXT.device();
    let queue = GPU_CONTEXT.queue();
    let cached = get_pipeline(msaa);
    let sample_count = cached.sample_count;

    let uniforms = [width as f32, height as f32, 0.0, 0.0];
    let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SDF 2D Uniforms"),
        contents: bytemuck::cast_slice(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let prim_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SDF 2D Primitives"),
        contents: bytemuck::cast_slice(primitives),
        usage: wgpu::BufferUsages::STORAGE,
    });

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

    // Atlas texture for PRIM_GLYPH (1px placeholder if no atlas provided)
    let (atlas_data, atlas_w, atlas_h) = match glyph_atlas {
        Some(a) => (&a.data[..], a.width, a.height),
        None => (&[128u8] as &[u8], 1u32, 1u32),
    };
    let atlas_tex = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("Glyph Atlas"),
            size: wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        atlas_data,
    );
    let atlas_view = atlas_tex.create_view(&Default::default());
    let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Glyph Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Atlas Bind Group"),
        layout: &cached.atlas_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&atlas_sampler),
            },
        ],
    });

    // Resolve texture (always 1x) — readback target
    let resolve_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SDF 2D Resolve"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let resolve_view = resolve_tex.create_view(&Default::default());

    // MSAA color texture (sample_count > 1 for anti-aliasing)
    let msaa_tex = if sample_count > 1 {
        Some(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SDF 2D MSAA"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        }))
    } else {
        None
    };

    let msaa_view = msaa_tex
        .as_ref()
        .map(|t| t.create_view(&Default::default()));
    let (color_view, resolve_target) = match &msaa_view {
        Some(mv) => (mv, Some(&resolve_view)),
        None => (&resolve_view, None),
    };

    let bytes_per_row = ((width * 4).div_ceil(256)) * 256;
    let readback_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("SDF 2D Readback"),
        size: (bytes_per_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SDF 2D Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target,
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
        pass.set_bind_group(1, &atlas_bind_group, &[]);
        pass.draw(0..6, 0..primitives.len() as u32);
    }

    // Copy resolved (1x) texture to readback buffer
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &resolve_tex,
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
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    GPU_CONTEXT.submit_and_poll(encoder.finish());

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
// Vector font — geometric sans-serif, uppercase A-Z + digits + punctuation
// Each glyph: line segments in a normalized 0.6 × 1.0 cell (top-left origin)
// ═══════════════════════════════════════════════════════════════════════════

fn glyph_strokes(ch: char) -> &'static [[f32; 4]] {
    match ch.to_ascii_uppercase() {
        'A' => &[
            [0.0, 1.0, 0.3, 0.0],
            [0.3, 0.0, 0.6, 1.0],
            [0.1, 0.6, 0.5, 0.6],
        ],
        'B' => &[
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.5, 0.0],
            [0.5, 0.0, 0.55, 0.12],
            [0.55, 0.12, 0.55, 0.38],
            [0.55, 0.38, 0.5, 0.5],
            [0.0, 0.5, 0.5, 0.5],
            [0.5, 0.5, 0.6, 0.62],
            [0.6, 0.62, 0.6, 0.88],
            [0.6, 0.88, 0.5, 1.0],
            [0.0, 1.0, 0.5, 1.0],
        ],
        'C' => &[
            [0.6, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.6, 1.0],
        ],
        'D' => &[
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.4, 0.0],
            [0.4, 0.0, 0.6, 0.2],
            [0.6, 0.2, 0.6, 0.8],
            [0.6, 0.8, 0.4, 1.0],
            [0.0, 1.0, 0.4, 1.0],
        ],
        'E' => &[
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.6, 0.0],
            [0.0, 0.5, 0.45, 0.5],
            [0.0, 1.0, 0.6, 1.0],
        ],
        'F' => &[
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.6, 0.0],
            [0.0, 0.5, 0.45, 0.5],
        ],
        'G' => &[
            [0.6, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.6, 1.0],
            [0.6, 1.0, 0.6, 0.5],
            [0.6, 0.5, 0.3, 0.5],
        ],
        'H' => &[
            [0.0, 0.0, 0.0, 1.0],
            [0.6, 0.0, 0.6, 1.0],
            [0.0, 0.5, 0.6, 0.5],
        ],
        'I' => &[
            [0.3, 0.0, 0.3, 1.0],
            [0.1, 0.0, 0.5, 0.0],
            [0.1, 1.0, 0.5, 1.0],
        ],
        'J' => &[
            [0.6, 0.0, 0.6, 0.85],
            [0.6, 0.85, 0.45, 1.0],
            [0.45, 1.0, 0.15, 1.0],
            [0.15, 1.0, 0.0, 0.85],
        ],
        'K' => &[
            [0.0, 0.0, 0.0, 1.0],
            [0.6, 0.0, 0.0, 0.5],
            [0.0, 0.5, 0.6, 1.0],
        ],
        'L' => &[[0.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.6, 1.0]],
        'M' => &[
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.3, 0.45],
            [0.3, 0.45, 0.6, 0.0],
            [0.6, 0.0, 0.6, 1.0],
        ],
        'N' => &[
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.6, 1.0],
            [0.6, 1.0, 0.6, 0.0],
        ],
        'O' => &[
            [0.0, 0.0, 0.0, 1.0],
            [0.6, 0.0, 0.6, 1.0],
            [0.0, 0.0, 0.6, 0.0],
            [0.0, 1.0, 0.6, 1.0],
        ],
        'P' => &[
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.55, 0.0],
            [0.55, 0.0, 0.55, 0.5],
            [0.55, 0.5, 0.0, 0.5],
        ],
        'Q' => &[
            [0.0, 0.0, 0.0, 1.0],
            [0.6, 0.0, 0.6, 1.0],
            [0.0, 0.0, 0.6, 0.0],
            [0.0, 1.0, 0.6, 1.0],
            [0.4, 0.75, 0.7, 1.05],
        ],
        'R' => &[
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.55, 0.0],
            [0.55, 0.0, 0.55, 0.5],
            [0.55, 0.5, 0.0, 0.5],
            [0.2, 0.5, 0.6, 1.0],
        ],
        'S' => &[
            [0.6, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.5],
            [0.0, 0.5, 0.6, 0.5],
            [0.6, 0.5, 0.6, 1.0],
            [0.6, 1.0, 0.0, 1.0],
        ],
        'T' => &[[0.0, 0.0, 0.6, 0.0], [0.3, 0.0, 0.3, 1.0]],
        'U' => &[
            [0.0, 0.0, 0.0, 1.0],
            [0.6, 0.0, 0.6, 1.0],
            [0.0, 1.0, 0.6, 1.0],
        ],
        'V' => &[[0.0, 0.0, 0.3, 1.0], [0.3, 1.0, 0.6, 0.0]],
        'W' => &[
            [0.0, 0.0, 0.15, 1.0],
            [0.15, 1.0, 0.3, 0.45],
            [0.3, 0.45, 0.45, 1.0],
            [0.45, 1.0, 0.6, 0.0],
        ],
        'X' => &[[0.0, 0.0, 0.6, 1.0], [0.6, 0.0, 0.0, 1.0]],
        'Y' => &[
            [0.0, 0.0, 0.3, 0.5],
            [0.6, 0.0, 0.3, 0.5],
            [0.3, 0.5, 0.3, 1.0],
        ],
        'Z' => &[
            [0.0, 0.0, 0.6, 0.0],
            [0.6, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.6, 1.0],
        ],
        '0' => &[
            [0.0, 0.0, 0.0, 1.0],
            [0.6, 0.0, 0.6, 1.0],
            [0.0, 0.0, 0.6, 0.0],
            [0.0, 1.0, 0.6, 1.0],
            [0.0, 1.0, 0.6, 0.0],
        ],
        '1' => &[
            [0.15, 0.15, 0.3, 0.0],
            [0.3, 0.0, 0.3, 1.0],
            [0.1, 1.0, 0.5, 1.0],
        ],
        '2' => &[
            [0.0, 0.0, 0.6, 0.0],
            [0.6, 0.0, 0.6, 0.5],
            [0.6, 0.5, 0.0, 0.5],
            [0.0, 0.5, 0.0, 1.0],
            [0.0, 1.0, 0.6, 1.0],
        ],
        '3' => &[
            [0.0, 0.0, 0.6, 0.0],
            [0.6, 0.0, 0.6, 1.0],
            [0.0, 1.0, 0.6, 1.0],
            [0.15, 0.5, 0.6, 0.5],
        ],
        '4' => &[
            [0.0, 0.0, 0.0, 0.5],
            [0.0, 0.5, 0.6, 0.5],
            [0.6, 0.0, 0.6, 1.0],
        ],
        '5' => &[
            [0.6, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.5],
            [0.0, 0.5, 0.6, 0.5],
            [0.6, 0.5, 0.6, 1.0],
            [0.6, 1.0, 0.0, 1.0],
        ],
        '6' => &[
            [0.6, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.6, 1.0],
            [0.6, 1.0, 0.6, 0.5],
            [0.6, 0.5, 0.0, 0.5],
        ],
        '7' => &[[0.0, 0.0, 0.6, 0.0], [0.6, 0.0, 0.3, 1.0]],
        '8' => &[
            [0.0, 0.0, 0.0, 1.0],
            [0.6, 0.0, 0.6, 1.0],
            [0.0, 0.0, 0.6, 0.0],
            [0.0, 1.0, 0.6, 1.0],
            [0.0, 0.5, 0.6, 0.5],
        ],
        '9' => &[
            [0.0, 0.5, 0.6, 0.5],
            [0.6, 0.0, 0.6, 1.0],
            [0.0, 0.0, 0.6, 0.0],
            [0.0, 0.0, 0.0, 0.5],
            [0.6, 1.0, 0.0, 1.0],
        ],
        '.' => &[[0.25, 0.9, 0.35, 1.0]],
        '!' => &[[0.3, 0.0, 0.3, 0.7], [0.27, 0.88, 0.33, 0.95]],
        '-' => &[[0.1, 0.5, 0.5, 0.5]],
        _ => &[], // space and unsupported
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Actor
// ═══════════════════════════════════════════════════════════════════════════

#[actor(
    Gpu2DRenderActor,
    inports::<100>(primitives, tick, values, atlas, metrics, atlas_size),
    outports::<1>(image, metadata),
    state(MemoryState)
)]
pub async fn gpu_2d_render_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(800) as u32;
    let height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(450) as u32;
    let bg = config
        .get("background")
        .and_then(|v| v.as_array())
        .map(|a| [fv(a, 0), fv(a, 1), fv(a, 2), fv(a, 3)])
        .unwrap_or([0.02, 0.01, 0.07, 1.0]);

    // Cache primitives from inport (fan-in: multiple shape sources accumulate)
    // Accepts:
    //   Single shape:   { "index": 0, "type": "rect", "bounds": [...], ... }
    //   Shape array:    [ { "type": "rect", ... }, { "type": "circle", ... } ]
    //   Keyed batch:    { "shapes": [ ... ] }
    if let Some(Message::Object(obj)) = payload.get("primitives") {
        let v: Value = obj.as_ref().clone().into();
        if let Some(arr) = v.get("shapes").and_then(|s| s.as_array()) {
            // Keyed batch: { "shapes": [...] }
            for (i, shape) in arr.iter().enumerate() {
                ctx.pool_upsert("_shapes", &format!("s{}", i), shape.clone());
            }
        } else if v.is_array() {
            // Direct array
            if let Some(arr) = v.as_array() {
                for (i, shape) in arr.iter().enumerate() {
                    ctx.pool_upsert("_shapes", &format!("s{}", i), shape.clone());
                }
            }
        } else if v.get("type").is_some() {
            // Single shape with optional "index" for ordering
            let idx = v
                .get("index")
                .and_then(|i| i.as_u64())
                .map(|i| format!("s{}", i))
                .unwrap_or_else(|| {
                    let n = ctx.get_pool("_shapes").len();
                    format!("s{}", n)
                });
            ctx.pool_upsert("_shapes", &idx, v);
        }
    }

    // Cache glyph atlas from GlyphAtlasActor (one-shot, persists across ticks)
    if let Some(Message::Bytes(bytes)) = payload.get("atlas") {
        ctx.pool_upsert("_atlas", "bitmap", json!(base64_encode(bytes)));
    }
    if let Some(Message::Object(obj)) = payload.get("metrics") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_atlas", "metrics", v);
    }
    if let Some(Message::Object(obj)) = payload.get("atlas_size") {
        let v: Value = obj.as_ref().clone().into();
        ctx.pool_upsert("_atlas", "size", v);
    }

    // ── Inline font loading: build atlas from text config font path ──
    // If no atlas cached yet and any text entry carries a "font" path,
    // build the atlas immediately without an external font/atlas pipeline.
    let atlas_empty = ctx.get_pool("_atlas").into_iter()
        .find(|(k, _)| k == "bitmap")
        .is_none();
    if atlas_empty {
        let text_cfgs = config.get("text").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        if let Some(font_path) = text_cfgs.iter().find_map(|t| t.get("font").and_then(|v| v.as_str()).map(|s| s.to_string())) {
            let font_size = text_cfgs.iter().find_map(|t| t.get("size").and_then(|v| v.as_f64())).unwrap_or(48.0) as f32;
            if let Ok(font_bytes) = std::fs::read(&font_path) {
                if let Ok(atlas) = super::font_atlas::get_or_build_atlas(&font_path, &font_bytes, font_size, true, "") {
                    ctx.pool_upsert("_atlas", "bitmap", json!(base64_encode(&atlas.bitmap)));
                    let mut metrics_map = serde_json::Map::new();
                    for (ch, info) in &atlas.glyphs {
                        metrics_map.insert(ch.to_string(), json!({
                            "x": info.atlas_x, "y": info.atlas_y,
                            "w": info.width, "h": info.height,
                            "advance": info.advance,
                            "bearing_x": info.bearing_x, "bearing_y": info.bearing_y,
                        }));
                    }
                    ctx.pool_upsert("_atlas", "metrics", Value::Object(metrics_map));
                    ctx.pool_upsert("_atlas", "size", json!([atlas.width, atlas.height]));
                }
            }
        }
    }

    // ── Parse animation values ──
    // Supports two sources:
    //   1. Timeline: full object { "s0_x": 400.0, "s0_y": 225.0, ... }
    //   2. KeyframeActor fan-in: individual { "s0_x": 400.0 } merged into pool
    let mut vals: HashMap<String, f64> = HashMap::new();
    if let Some(Message::Object(obj)) = payload.get("values") {
        let v: Value = obj.as_ref().clone().into();
        if let Some(map) = v.as_object() {
            for (k, val) in map {
                if let Some(f) = val.as_f64() {
                    // Pool each named value individually for fan-in accumulation
                    ctx.pool_upsert("_vals", k, json!(f));
                    vals.insert(k.clone(), f);
                }
            }
        }
    }
    // Restore all pooled values (includes fan-in accumulation from prior messages)
    for (k, stored) in ctx.get_pool("_vals") {
        if !vals.contains_key(&k) {
            if let Some(f) = stored.as_f64() {
                vals.insert(k, f);
            }
        }
    }

    // Only render when values or tick arrives
    if !payload.contains_key("values") && !payload.contains_key("tick") {
        return Ok(HashMap::new());
    }

    let get_val = |prefix: &str, prop: &str| -> Option<f64> {
        vals.get(&format!("{}_{}", prefix, prop)).copied()
    };

    // Resolve shapes: pooled from inport (fan-in) takes precedence over config
    let pooled_shapes: Vec<(String, Value)> = ctx.get_pool("_shapes").into_iter().collect();
    let shapes: Vec<Value> = if !pooled_shapes.is_empty() {
        let mut sorted = pooled_shapes;
        sorted.sort_by(|(a, _), (b, _)| a.cmp(b));
        sorted.into_iter().map(|(_, v)| v).collect()
    } else {
        config
            .get("shapes")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    };
    let text_configs = config
        .get("text")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Two-phase primitive list: shadows before fills so no shadow ever
    // composites on top of a fill, regardless of shape declaration order.
    let mut shadow_prims: Vec<GpuPrimitive> = Vec::new();
    let mut fill_prims: Vec<GpuPrimitive> = Vec::new();

    // ── Geometric shapes (sN_ prefix) ──
    for (idx, prim_json) in shapes.iter().enumerate() {
        let pfx = format!("s{}", idx);
        let anim_x = get_val(&pfx, "x");
        let anim_y = get_val(&pfx, "y");
        let anim_scale = get_val(&pfx, "scale").unwrap_or(1.0);
        let anim_rotation = get_val(&pfx, "rotation").unwrap_or(0.0);
        let anim_opacity = get_val(&pfx, "opacity").unwrap_or(1.0);

        if anim_scale.abs() < 0.001 || anim_opacity < 0.01 {
            continue;
        }

        let ptype = prim_json
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("rect");
        let base_bounds = prim_json
            .get("bounds")
            .and_then(|v| v.as_array())
            .map(|a| [fv(a, 0), fv(a, 1), fv(a, 2), fv(a, 3)])
            .unwrap_or([0.0, 0.0, 100.0, 100.0]);

        let w = base_bounds[2] * anim_scale as f32;
        let h = base_bounds[3] * anim_scale as f32;
        let x = anim_x.map(|v| v as f32 - w / 2.0).unwrap_or(base_bounds[0]);
        let y = anim_y.map(|v| v as f32 - h / 2.0).unwrap_or(base_bounds[1]);

        let mut color = prim_json
            .get("color")
            .and_then(|v| v.as_array())
            .map(|a| [fv(a, 0), fv(a, 1), fv(a, 2), fv(a, 3)])
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        color[3] *= anim_opacity as f32;

        let mut p = match ptype {
            "circle" => GpuPrimitive::circle(x + w / 2.0, y + h / 2.0, w.min(h) / 2.0, color),
            _ => {
                let r = prim_json
                    .get("cornerRadius")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32;
                GpuPrimitive::rect(x, y, w, h, color, r)
            }
        };

        if anim_rotation.abs() > 0.001 {
            p = p.with_rotation(anim_rotation as f32);
        }
        if let Some(shadow) = prim_json.get("shadow") {
            let sx = shadow.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let sy = shadow.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let blur = shadow.get("blur").and_then(|v| v.as_f64()).unwrap_or(10.0) as f32;
            let sc = shadow
                .get("color")
                .and_then(|v| v.as_array())
                .map(|a| [fv(a, 0), fv(a, 1), fv(a, 2), fv(a, 3)])
                .unwrap_or([0.0, 0.0, 0.0, 0.5]);
            p = p.with_shadow(sx, sy, blur, sc);
        }
        if let Some(border) = prim_json.get("border") {
            let bw = border.get("width").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
            let bc = border
                .get("color")
                .and_then(|v| v.as_array())
                .map(|a| [fv(a, 0), fv(a, 1), fv(a, 2), fv(a, 3)])
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            p = p.with_border(bw, bc);
        }

        // Shadow pre-pass: emit shadow-only primitive before the fill so that
        // shadows from later shapes never composite on top of earlier fills.
        if p.shadow[2] > 0.001 || p.shadow[0].abs() > 0.001 || p.shadow[1].abs() > 0.001 {
            shadow_prims.push(p.as_shadow_only());
        }
        fill_prims.push(p.clear_shadow());
    }

    // ── Load cached glyph atlas from pool ──
    let atlas_pool: HashMap<String, Value> = ctx.get_pool("_atlas").into_iter().collect();
    let glyph_metrics: Option<&Value> = atlas_pool.get("metrics");
    let atlas_size: Option<(u32, u32)> = atlas_pool.get("size").and_then(|v| {
        let arr = v.as_array()?;
        Some((arr.first()?.as_u64()? as u32, arr.get(1)?.as_u64()? as u32))
    });
    let has_atlas = glyph_metrics.is_some() && atlas_size.is_some();

    // ── Text (cN_ prefix per character) ──
    for text_cfg in &text_configs {
        let content = text_cfg
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let tx = text_cfg.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let ty = text_cfg.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let size = text_cfg
            .get("size")
            .and_then(|v| v.as_f64())
            .unwrap_or(48.0) as f32;
        let base_color = text_cfg
            .get("color")
            .and_then(|v| v.as_array())
            .map(|a| [fv(a, 0), fv(a, 1), fv(a, 2), fv(a, 3)])
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let tracking = text_cfg
            .get("tracking")
            .and_then(|v| v.as_f64())
            .unwrap_or(6.0) as f32;
        let centered = text_cfg
            .get("center")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Compute character advance using atlas metrics or fallback
        let font_scale = if has_atlas {
            // Atlas was rasterized at a specific size; scale to requested size
            let atlas_font_size = glyph_metrics
                .and_then(|m| {
                    // Estimate atlas font size from a known glyph (e.g., 'H')
                    m.get("H").and_then(|g| g.get("h")).and_then(|v| v.as_f64())
                })
                .unwrap_or(size as f64) as f32;
            size / atlas_font_size.max(1.0)
        } else {
            1.0
        };

        // Layout: compute per-char x positions using advance from metrics
        let mut char_positions: Vec<(usize, char, f32)> = Vec::new();
        let mut cursor_x = 0.0f32;
        for (ci, ch) in content.chars().enumerate() {
            char_positions.push((ci, ch, cursor_x));
            let adv = if has_atlas {
                glyph_metrics
                    .and_then(|m| m.get(&ch.to_string()))
                    .and_then(|g| g.get("advance"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(size as f64 * 0.6) as f32
                    * font_scale
            } else {
                size * 0.6
            };
            cursor_x += adv + tracking;
        }
        let total_w = cursor_x - tracking;
        let start_x = if centered { tx - total_w / 2.0 } else { tx };

        for &(ci, ch, cx) in &char_positions {
            if ch == ' ' {
                continue;
            }
            let pfx = format!("c{}", ci);
            let char_scale = get_val(&pfx, "scale").unwrap_or(1.0);
            let char_opacity = get_val(&pfx, "opacity").unwrap_or(1.0);
            let char_y_off = get_val(&pfx, "y").unwrap_or(0.0) as f32;

            if char_scale < 0.01 || char_opacity < 0.01 {
                continue;
            }

            let mut color = base_color;
            color[3] *= char_opacity as f32;

            if has_atlas {
                // ── PRIM_GLYPH path: textured SDF quad ──
                let (aw, ah) = atlas_size.unwrap();
                if let Some(glyph) = glyph_metrics.and_then(|m| m.get(&ch.to_string())) {
                    let gx = glyph.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let gy = glyph.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let gw = glyph.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let gh = glyph.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let bearing_x = glyph
                        .get("bearing_x")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as f32;
                    let bearing_y = glyph
                        .get("bearing_y")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as f32;

                    let s = char_scale as f32;
                    let qw = gw * font_scale * s;
                    let qh = gh * font_scale * s;

                    // Position: baseline at ty + size*0.8 (typical ascent ratio),
                    // then offset by font metrics. bearing_y = ymin (from baseline).
                    // In fontdue: glyph top = baseline - (gh - bearing_y) in glyph coords.
                    let baseline_y = ty + size * 0.78 + char_y_off;
                    let qx = start_x + cx + bearing_x * font_scale * s;
                    let qy = baseline_y - (gh as f32 + bearing_y) * font_scale * s;

                    // UV in atlas (normalized 0..1)
                    let u0 = gx / aw as f32;
                    let v0 = gy / ah as f32;
                    let u1 = (gx + gw) / aw as f32;
                    let v1 = (gy + gh) / ah as f32;

                    fill_prims.push(GpuPrimitive::glyph(qx, qy, qw, qh, [u0, v0, u1, v1], color));
                }
            } else {
                // ── Fallback: geometric segment font ──
                let char_w = size * 0.6;
                let ccx = start_x + cx + char_w * 0.5;
                let ccy = ty + size * 0.5 + char_y_off;
                let thickness = size * 0.055;
                let s = char_scale as f32;
                let hw = char_w * 0.5;
                let hs = size * 0.5;
                let strokes = glyph_strokes(ch);
                for seg in strokes {
                    let sx1 = ccx + (seg[0] * size - hw) * s;
                    let sy1 = ccy + (seg[1] * size - hs) * s;
                    let sx2 = ccx + (seg[2] * size - hw) * s;
                    let sy2 = ccy + (seg[3] * size - hs) * s;
                    fill_prims.push(GpuPrimitive::segment(
                        sx1,
                        sy1,
                        sx2,
                        sy2,
                        thickness * s,
                        color,
                    ));
                }
            }
        }
    }

    // Assemble: shadow-only pass first, then fills — correct layer ordering.
    let mut gpu_prims: Vec<GpuPrimitive> = shadow_prims;
    gpu_prims.extend(fill_prims);

    if gpu_prims.is_empty() {
        return Ok(HashMap::new());
    }

    // Build GPU atlas from cached pool data
    let atlas_gpu: Option<GlyphAtlasGpu> = atlas_pool.get("bitmap").and_then(|v| {
        let data = base64_decode(v.as_str()?)?;
        let (w, h) = atlas_size?;
        Some(GlyphAtlasGpu {
            data,
            width: w,
            height: h,
        })
    });

    let msaa = config.get("msaa").and_then(|v| v.as_u64()).unwrap_or(4) as u32;

    #[cfg(feature = "gpu")]
    let rgba = render_2d(&gpu_prims, width, height, bg, atlas_gpu.as_ref(), msaa);

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

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        out.push(TABLE[(b0 >> 2) as usize]);
        out.push(TABLE[(((b0 & 3) << 4) | (b1 >> 4)) as usize]);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0xF) << 2) | (b2 >> 6)) as usize]);
        } else {
            out.push(b'=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3F) as usize]);
        } else {
            out.push(b'=');
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut i = 0;
    while i + 3 < bytes.len() {
        let a = b64val(bytes[i])?;
        let b = b64val(bytes[i + 1])?;
        let c = b64val(bytes[i + 2])?;
        let d = b64val(bytes[i + 3])?;
        out.push((a << 2) | (b >> 4));
        if bytes[i + 2] != b'=' {
            out.push((b << 4) | (c >> 2));
        }
        if bytes[i + 3] != b'=' {
            out.push((c << 6) | d);
        }
        i += 4;
    }
    Some(out)
}

fn b64val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        b'=' => Some(0),
        _ => None,
    }
}
