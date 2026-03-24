//! WGSL code generation from ShaderNode IR.
//!
//! Walks the ShaderNode tree and emits vertex + fragment shader pair
//! with Cook-Torrance PBR lighting.

use crate::ir::*;
use std::collections::HashSet;

/// Compilation context — tracks variable names, texture slots, and features.
struct Ctx {
    var_counter: usize,
    code: String,
    textures: Vec<TextureSlot>,
    texture_ids: HashSet<String>,
    needs_uv: bool,
    needs_tangent: bool,
    needs_time: bool,
    needs_camera_pos: bool,
}

impl Ctx {
    fn new() -> Self {
        Self {
            var_counter: 0,
            code: String::new(),
            textures: Vec::new(),
            texture_ids: HashSet::new(),
            needs_uv: false,
            needs_tangent: false,
            needs_time: false,
            needs_camera_pos: false,
        }
    }

    fn fresh_var(&mut self) -> String {
        let v = format!("v{}", self.var_counter);
        self.var_counter += 1;
        v
    }

    fn emit(&mut self, line: &str) {
        self.code.push_str("    ");
        self.code.push_str(line);
        self.code.push('\n');
    }

    fn add_texture(&mut self, asset_id: &str, label: &str) -> u32 {
        if let Some(slot) = self.textures.iter().find(|t| t.asset_id == asset_id) {
            return slot.binding;
        }
        let binding = (self.textures.len() as u32) + 2; // 0=scene, 1=material
        self.textures.push(TextureSlot {
            binding,
            asset_id: asset_id.to_string(),
            label: label.to_string(),
        });
        self.texture_ids.insert(asset_id.to_string());
        binding
    }
}

/// Emit a ShaderNode, returning the WGSL variable name holding the result.
/// Result type varies: "f32", "vec3f", "vec4f" depending on node.
fn emit_node(ctx: &mut Ctx, node: &ShaderNode) -> String {
    match node {
        // ═══ Constants ═══
        ShaderNode::ConstFloat(v) => format!("{:.6}", v),
        ShaderNode::ConstVec3(v) => format!("vec3f({:.6}, {:.6}, {:.6})", v[0], v[1], v[2]),
        ShaderNode::ConstVec4(v) => {
            format!("vec4f({:.6}, {:.6}, {:.6}, {:.6})", v[0], v[1], v[2], v[3])
        }

        // ═══ Inputs ═══
        ShaderNode::TexCoord => {
            ctx.needs_uv = true;
            "in.uv".to_string()
        }
        ShaderNode::ObjectPosition => "in.world_pos".to_string(),
        ShaderNode::ObjectNormal => "in.world_normal".to_string(),
        ShaderNode::CameraVector => {
            ctx.needs_camera_pos = true;
            "normalize(u_scene.camera_pos - in.world_pos)".to_string()
        }
        ShaderNode::VertexColor => "in.vertex_color".to_string(),
        ShaderNode::Tangent => {
            ctx.needs_tangent = true;
            "in.tangent".to_string()
        }
        ShaderNode::Time => {
            ctx.needs_time = true;
            "u_scene.time".to_string()
        }

        // ═══ Math ═══
        ShaderNode::MathOp { op, a, b } => {
            let va = emit_node(ctx, a);
            let vb = b.as_ref().map(|n| emit_node(ctx, n));
            let var = ctx.fresh_var();
            let expr = match (op, vb.as_deref()) {
                (MathOpType::Add, Some(vb)) => format!("({va} + {vb})"),
                (MathOpType::Subtract, Some(vb)) => format!("({va} - {vb})"),
                (MathOpType::Multiply, Some(vb)) => format!("({va} * {vb})"),
                (MathOpType::Divide, Some(vb)) => format!("({va} / max({vb}, 0.00001))"),
                (MathOpType::Power, Some(vb)) => format!("pow({va}, {vb})"),
                (MathOpType::Min, Some(vb)) => format!("min({va}, {vb})"),
                (MathOpType::Max, Some(vb)) => format!("max({va}, {vb})"),
                (MathOpType::Modulo, Some(vb)) => format!("({va} % {vb})"),
                (MathOpType::Atan2, Some(vb)) => format!("atan2({va}, {vb})"),
                (MathOpType::Smoothstep, Some(vb)) => format!("smoothstep(0.0, 1.0, {va})"),
                (MathOpType::Lerp, Some(vb)) => format!("mix({va}, {vb}, 0.5)"),
                (MathOpType::Step, Some(vb)) => format!("step({va}, {vb})"),
                (MathOpType::Dot, Some(vb)) => format!("dot({va}, {vb})"),
                (MathOpType::Cross, Some(vb)) => format!("cross({va}, {vb})"),
                (MathOpType::Distance, Some(vb)) => format!("distance({va}, {vb})"),
                (MathOpType::Reflect, Some(vb)) => format!("reflect({va}, {vb})"),
                (MathOpType::Sqrt, _) => format!("sqrt(abs({va}))"),
                (MathOpType::Abs, _) => format!("abs({va})"),
                (MathOpType::Sin, _) => format!("sin({va})"),
                (MathOpType::Cos, _) => format!("cos({va})"),
                (MathOpType::Tan, _) => format!("tan({va})"),
                (MathOpType::Asin, _) => format!("asin(clamp({va}, -1.0, 1.0))"),
                (MathOpType::Acos, _) => format!("acos(clamp({va}, -1.0, 1.0))"),
                (MathOpType::Floor, _) => format!("floor({va})"),
                (MathOpType::Ceil, _) => format!("ceil({va})"),
                (MathOpType::Fract, _) => format!("fract({va})"),
                (MathOpType::Sign, _) => format!("sign({va})"),
                (MathOpType::Log, _) => format!("log({va})"),
                (MathOpType::Exp, _) => format!("exp({va})"),
                (MathOpType::Normalize, _) => format!("normalize({va})"),
                (MathOpType::Length, _) => format!("length({va})"),
                (MathOpType::Negate, _) => format!("(-{va})"),
                (MathOpType::Invert, _) => format!("(1.0 - {va})"),
                _ => va.clone(),
            };
            ctx.emit(&format!("let {var} = {expr};"));
            var
        }

        // ═══ Color mix ═══
        ShaderNode::ColorMix { mode, fac, a, b } => {
            let vf = emit_node(ctx, fac);
            let va = emit_node(ctx, a);
            let vb = emit_node(ctx, b);
            let var = ctx.fresh_var();
            let expr = match mode {
                MixMode::Mix => format!("mix({va}, {vb}, {vf})"),
                MixMode::Add => format!("({va} + {vb} * {vf})"),
                MixMode::Multiply => format!("mix({va}, {va} * {vb}, {vf})"),
                MixMode::Screen => format!("mix({va}, 1.0 - (1.0 - {va}) * (1.0 - {vb}), {vf})"),
                MixMode::Overlay => {
                    format!("mix({va}, mix(2.0*{va}*{vb}, 1.0-2.0*(1.0-{va})*(1.0-{vb}), step(0.5,{va})), {vf})")
                }
                MixMode::Darken => format!("mix({va}, min({va}, {vb}), {vf})"),
                MixMode::Lighten => format!("mix({va}, max({va}, {vb}), {vf})"),
                MixMode::Difference => format!("mix({va}, abs({va} - {vb}), {vf})"),
                MixMode::Subtract => format!("mix({va}, {va} - {vb}, {vf})"),
                _ => format!("mix({va}, {vb}, {vf})"),
            };
            ctx.emit(&format!("let {var} = {expr};"));
            var
        }

        // ═══ Color ramp ═══
        ShaderNode::ColorRamp { stops, input } => {
            let vi = emit_node(ctx, input);
            let var = ctx.fresh_var();
            if stops.len() < 2 {
                let c = stops.first().map(|s| s.color).unwrap_or([1.0, 1.0, 1.0, 1.0]);
                ctx.emit(&format!(
                    "let {var} = vec3f({:.4}, {:.4}, {:.4});",
                    c[0], c[1], c[2]
                ));
            } else {
                // Emit piecewise linear interpolation
                ctx.emit(&format!("var {var} = vec3f(0.0);"));
                for i in 0..stops.len() - 1 {
                    let s0 = &stops[i];
                    let s1 = &stops[i + 1];
                    let c0 = s0.color;
                    let c1 = s1.color;
                    ctx.emit(&format!(
                        "if ({vi} >= {:.4} && {vi} <= {:.4}) {{ let t = ({vi} - {:.4}) / {:.4}; {var} = mix(vec3f({:.4},{:.4},{:.4}), vec3f({:.4},{:.4},{:.4}), t); }}",
                        s0.position, s1.position, s0.position,
                        (s1.position - s0.position).max(0.0001),
                        c0[0], c0[1], c0[2], c1[0], c1[1], c1[2]
                    ));
                }
            }
            var
        }

        // ═══ Separate/Combine XYZ ═══
        ShaderNode::SeparateXYZ { input, component } => {
            let vi = emit_node(ctx, input);
            match component.as_str() {
                "x" => format!("{vi}.x"),
                "y" => format!("{vi}.y"),
                "z" => format!("{vi}.z"),
                _ => format!("{vi}.x"),
            }
        }
        ShaderNode::CombineXYZ { x, y, z } => {
            let vx = emit_node(ctx, x);
            let vy = emit_node(ctx, y);
            let vz = emit_node(ctx, z);
            let var = ctx.fresh_var();
            ctx.emit(&format!("let {var} = vec3f({vx}, {vy}, {vz});"));
            var
        }

        // ═══ Fresnel ═══
        ShaderNode::Fresnel { ior } => {
            let vi = emit_node(ctx, ior);
            ctx.needs_camera_pos = true;
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var} = pow(1.0 - max(dot(normalize(in.world_normal), normalize(u_scene.camera_pos - in.world_pos)), 0.0), 5.0) * (1.0 - {vi}) + {vi};"
            ));
            var
        }

        // ═══ Clamp / MapRange ═══
        ShaderNode::Clamp { input, min_val, max_val } => {
            let vi = emit_node(ctx, input);
            let var = ctx.fresh_var();
            ctx.emit(&format!("let {var} = clamp({vi}, {:.6}, {:.6});", min_val, max_val));
            var
        }
        ShaderNode::MapRange { input, from_min, from_max, to_min, to_max } => {
            let vi = emit_node(ctx, input);
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var} = {:.6} + ({vi} - {:.6}) / {:.6} * {:.6};",
                to_min, from_min, (from_max - from_min).max(0.0001), to_max - to_min
            ));
            var
        }

        // ═══ Procedural textures ═══
        ShaderNode::NoiseTexture { scale, detail, roughness } => {
            let vs = emit_node(ctx, scale);
            ctx.needs_uv = true;
            let var = ctx.fresh_var();
            ctx.emit(&format!("let {var} = vec3f(fbm_noise(in.world_pos * {vs}));"));
            var
        }
        ShaderNode::CheckerTexture { scale, color1, color2 } => {
            let vs = emit_node(ctx, scale);
            let vc1 = emit_node(ctx, color1);
            let vc2 = emit_node(ctx, color2);
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var} = select({vc1}, {vc2}, (floor(in.world_pos.x * {vs}) + floor(in.world_pos.y * {vs}) + floor(in.world_pos.z * {vs})) % 2.0 < 1.0);"
            ));
            var
        }
        ShaderNode::VoronoiTexture { scale, .. } => {
            let vs = emit_node(ctx, scale);
            let var = ctx.fresh_var();
            ctx.emit(&format!("let {var} = vec3f(voronoi_noise(in.world_pos * {vs}));"));
            var
        }

        // ═══ Image texture ═══
        ShaderNode::ImageTexture { asset_id, uv } => {
            let vuv = emit_node(ctx, uv);
            let binding = ctx.add_texture(asset_id, "diffuse");
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var} = textureSample(tex_b{binding}, samp_b{binding}, {vuv}).rgb;"
            ));
            var
        }

        // ═══ Normal map ═══
        ShaderNode::NormalMap { strength, color } => {
            let vs = emit_node(ctx, strength);
            let vc = emit_node(ctx, color);
            ctx.needs_tangent = true;
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var}_raw = {vc} * 2.0 - 1.0; let {var} = normalize(in.world_normal + {var}_raw * {vs});"
            ));
            var
        }

        // ═══ Bump map ═══
        ShaderNode::BumpMap { strength, height } => {
            let vs = emit_node(ctx, strength);
            let vh = emit_node(ctx, height);
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var} = normalize(in.world_normal + vec3f(dpdx({vh}), dpdy({vh}), 0.0) * {vs});"
            ));
            var
        }

        // ═══ Principled BSDF — multi-light PBR shading ═══
        ShaderNode::PrincipledBsdf {
            base_color, metallic, roughness, normal,
            emission, emission_strength, ao, alpha, ..
        } => {
            let vbc = emit_node(ctx, base_color);
            let vm = emit_node(ctx, metallic);
            let vr = emit_node(ctx, roughness);
            let ve = emit_node(ctx, emission);
            let ves = emit_node(ctx, emission_strength);
            let va = ao.as_ref().map(|n| emit_node(ctx, n)).unwrap_or_else(|| "1.0".to_string());
            let _valpha = emit_node(ctx, alpha);
            let vn = normal
                .as_ref()
                .map(|n| emit_node(ctx, n))
                .unwrap_or_else(|| "normalize(in.world_normal)".to_string());

            ctx.needs_camera_pos = true;
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var} = pbr_shade_multi({vbc}, {vm}, {vr}, {vn}, normalize(u_scene.camera_pos - in.world_pos), in.world_pos, {va}, {ve} * {ves});"
            ));
            var
        }

        // ═══ Material output ═══
        ShaderNode::MaterialOutput { surface } => {
            emit_node(ctx, surface)
        }

        // ═══ Fallback ═══
        _ => "vec3f(0.8, 0.0, 0.8)".to_string(), // magenta = unimplemented
    }
}

/// Compile a ShaderNode tree into a complete WGSL material.
pub fn compile(root: &ShaderNode) -> CompiledMaterial {
    let mut ctx = Ctx::new();

    // Emit material evaluation code
    let result_var = emit_node(&mut ctx, root);

    // Build fragment shader
    let mut frag = String::new();
    frag.push_str(PBR_FUNCTIONS);
    frag.push_str(NOISE_FUNCTIONS);

    // Scene + light uniforms
    frag.push_str(UNIFORM_STRUCTS);

    // Texture bindings
    for slot in &ctx.textures {
        frag.push_str(&format!(
            "@group(0) @binding({b}) var tex_b{b}: texture_2d<f32>;\n\
             @group(0) @binding({sb}) var samp_b{b}: sampler;\n",
            b = slot.binding,
            sb = slot.binding + 100, // sampler bindings offset
        ));
    }
    frag.push('\n');

    // Fragment input/output
    frag.push_str("struct FragInput {\n");
    frag.push_str("    @location(0) world_pos: vec3f,\n");
    frag.push_str("    @location(1) world_normal: vec3f,\n");
    if ctx.needs_uv {
        frag.push_str("    @location(2) uv: vec2f,\n");
    }
    frag.push_str("};\n\n");

    // Fragment function
    frag.push_str("@fragment\nfn fs_main(in: FragInput) -> @location(0) vec4f {\n");
    frag.push_str(&ctx.code);
    frag.push_str(&format!("    return vec4f({result_var}, 1.0);\n"));
    frag.push_str("}\n");

    // Build vertex shader
    let vertex_wgsl = build_vertex_shader(ctx.needs_uv, ctx.needs_tangent);

    // Compute pipeline hash
    let hash = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        frag.hash(&mut h);
        vertex_wgsl.hash(&mut h);
        h.finish()
    };

    CompiledMaterial {
        vertex_wgsl,
        fragment_wgsl: frag,
        vertex_stride: if ctx.needs_uv { 32 } else { 24 },
        vertex_attributes: vec![],
        texture_slots: ctx.textures,
        base_color: [0.8, 0.8, 0.8, 1.0],
        metallic: 0.0,
        roughness: 0.5,
        emission_color: [0.0, 0.0, 0.0],
        emission_strength: 0.0,
        ao_strength: 1.0,
        pipeline_hash: hash,
    }
}

fn build_vertex_shader(needs_uv: bool, _needs_tangent: bool) -> String {
    let mut vs = String::new();
    vs.push_str(UNIFORM_STRUCTS);
    vs.push_str("struct VertexInput {\n");
    vs.push_str("    @location(0) position: vec3f,\n");
    vs.push_str("    @location(1) normal: vec3f,\n");
    if needs_uv {
        vs.push_str("    @location(2) uv: vec2f,\n");
    }
    vs.push_str("};\n\n");
    vs.push_str("struct VertexOutput {\n");
    vs.push_str("    @builtin(position) clip_pos: vec4f,\n");
    vs.push_str("    @location(0) world_pos: vec3f,\n");
    vs.push_str("    @location(1) world_normal: vec3f,\n");
    if needs_uv {
        vs.push_str("    @location(2) uv: vec2f,\n");
    }
    vs.push_str("};\n\n");
    vs.push_str("@vertex\nfn vs_main(in: VertexInput) -> VertexOutput {\n");
    vs.push_str("    var out: VertexOutput;\n");
    vs.push_str("    out.clip_pos = u_scene.view_proj * vec4f(in.position, 1.0);\n");
    vs.push_str("    out.world_pos = in.position;\n");
    vs.push_str("    out.world_normal = in.normal;\n");
    if needs_uv {
        vs.push_str("    out.uv = in.uv;\n");
    }
    vs.push_str("    return out;\n}\n");
    vs
}

// ═══════════════════════════════════════════════════════════════
// PBR lighting functions (Cook-Torrance)
// ═══════════════════════════════════════════════════════════════

const UNIFORM_STRUCTS: &str = r#"
struct Light {
    position: vec3f,
    light_type: f32,   // 0=directional, 1=point, 2=spot, 3=ambient
    direction: vec3f,
    intensity: f32,
    color: vec3f,
    range: f32,
    inner_cos: f32,
    outer_cos: f32,
    cast_shadow: f32,
    _pad: f32,
};

struct SceneUniforms {
    view_proj: mat4x4f,
    light_dir: vec3f,      // legacy fallback directional
    _pad: f32,
    camera_pos: vec3f,
    time: f32,
    light_count: u32,
    _pad2: vec3<u32>,
};

@group(0) @binding(0) var<uniform> u_scene: SceneUniforms;
@group(0) @binding(1) var<storage, read> u_lights: array<Light>;
"#;

const PBR_FUNCTIONS: &str = r#"
// GGX/Trowbridge-Reitz Normal Distribution Function
fn D_GGX(NoH: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = (NoH * NoH) * (a2 - 1.0) + 1.0;
    return a2 / (3.14159265 * d * d + 0.00001);
}

// Schlick-GGX Geometry Function
fn G_SchlickGGX(NdotV: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return NdotV / (NdotV * (1.0 - k) + k);
}

fn G_Smith(NoV: f32, NoL: f32, roughness: f32) -> f32 {
    return G_SchlickGGX(NoV, roughness) * G_SchlickGGX(NoL, roughness);
}

// Schlick Fresnel
fn F_Schlick(cosTheta: f32, F0: vec3f) -> vec3f {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

// Single-light PBR (used internally by multi-light loop)
fn pbr_direct(
    base_color: vec3f, metallic: f32, roughness: f32,
    N: vec3f, V: vec3f, L: vec3f, radiance: vec3f,
) -> vec3f {
    let H = normalize(V + L);
    let NoL = max(dot(N, L), 0.0);
    let NoV = max(dot(N, V), 0.001);
    let NoH = max(dot(N, H), 0.0);
    let HoV = max(dot(H, V), 0.0);

    let F0 = mix(vec3f(0.04), base_color, metallic);

    let D = D_GGX(NoH, max(roughness, 0.04));
    let G = G_Smith(NoV, NoL, max(roughness, 0.04));
    let F = F_Schlick(HoV, F0);

    let numerator = D * G * F;
    let denominator = 4.0 * NoV * NoL + 0.0001;
    let specular = numerator / denominator;

    let kS = F;
    let kD = (vec3f(1.0) - kS) * (1.0 - metallic);
    let diffuse = kD * base_color / 3.14159265;

    return (diffuse + specular) * radiance * NoL;
}

// Multi-light PBR: loops over light array, handles directional/point/spot
fn pbr_shade_multi(
    base_color: vec3f, metallic: f32, roughness: f32,
    N: vec3f, V: vec3f, world_pos: vec3f,
    ao: f32, emission: vec3f
) -> vec3f {
    var result = vec3f(0.0);
    let light_count = u_scene.light_count;

    // If no lights in array, use legacy single directional light
    if light_count == 0u {
        let L = normalize(u_scene.light_dir);
        result += pbr_direct(base_color, metallic, roughness, N, V, L, vec3f(1.0));
    }

    for (var i = 0u; i < min(light_count, 16u); i++) {
        let light = u_lights[i];
        let light_color = light.color * light.intensity;

        if light.light_type < 0.5 {
            // Directional light
            let L = normalize(-light.direction);
            result += pbr_direct(base_color, metallic, roughness, N, V, L, light_color);
        } else if light.light_type < 1.5 {
            // Point light
            let to_light = light.position - world_pos;
            let dist = length(to_light);
            let L = normalize(to_light);
            let attenuation = max(1.0 - dist / light.range, 0.0);
            let atten2 = attenuation * attenuation;
            result += pbr_direct(base_color, metallic, roughness, N, V, L, light_color * atten2);
        } else if light.light_type < 2.5 {
            // Spot light
            let to_light = light.position - world_pos;
            let dist = length(to_light);
            let L = normalize(to_light);
            let theta = dot(L, normalize(-light.direction));
            let epsilon = light.inner_cos - light.outer_cos;
            let spot_atten = clamp((theta - light.outer_cos) / max(epsilon, 0.001), 0.0, 1.0);
            let dist_atten = max(1.0 - dist / light.range, 0.0);
            let atten = spot_atten * dist_atten * dist_atten;
            result += pbr_direct(base_color, metallic, roughness, N, V, L, light_color * atten);
        } else {
            // Ambient light
            result += base_color * light_color * ao;
        }
    }

    // Hemisphere ambient (fallback when no ambient light in array)
    let ambient_up = vec3f(0.08, 0.10, 0.12);
    let ambient_down = vec3f(0.04, 0.03, 0.02);
    let ambient = mix(ambient_down, ambient_up, dot(N, vec3f(0.0, 1.0, 0.0)) * 0.5 + 0.5) * base_color * ao;

    return ambient + result + emission;
}

// Legacy single-light PBR (backward compat)
fn pbr_shade(
    base_color: vec3f, metallic: f32, roughness: f32,
    N: vec3f, V: vec3f, L: vec3f, light_color: vec3f,
    ao: f32, emission: vec3f
) -> vec3f {
    let direct = pbr_direct(base_color, metallic, roughness, N, V, L, light_color);
    let ambient_up = vec3f(0.08, 0.10, 0.12);
    let ambient_down = vec3f(0.04, 0.03, 0.02);
    let ambient = mix(ambient_down, ambient_up, dot(N, vec3f(0.0, 1.0, 0.0)) * 0.5 + 0.5) * base_color * ao;
    return ambient + direct + emission;
}
"#;

const NOISE_FUNCTIONS: &str = r#"
// Simple hash-based noise for procedural textures
fn hash31(p: vec3f) -> f32 {
    var p3 = fract(p * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn noise3(p: vec3f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(mix(hash31(i), hash31(i + vec3f(1,0,0)), u.x),
            mix(hash31(i + vec3f(0,1,0)), hash31(i + vec3f(1,1,0)), u.x), u.y),
        mix(mix(hash31(i + vec3f(0,0,1)), hash31(i + vec3f(1,0,1)), u.x),
            mix(hash31(i + vec3f(0,1,1)), hash31(i + vec3f(1,1,1)), u.x), u.y), u.z);
}

fn fbm_noise(p: vec3f) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var pos = p;
    for (var i = 0; i < 5; i++) {
        val += amp * noise3(pos);
        pos *= 2.0;
        amp *= 0.5;
    }
    return val;
}

fn voronoi_noise(p: vec3f) -> f32 {
    let n = floor(p);
    let f = fract(p);
    var md = 8.0;
    for (var i = -1; i <= 1; i++) {
        for (var j = -1; j <= 1; j++) {
            for (var k = -1; k <= 1; k++) {
                let g = vec3f(f32(i), f32(j), f32(k));
                let o = vec3f(hash31(n + g), hash31(n + g + 31.0), hash31(n + g + 57.0));
                let r = g + o - f;
                let d = dot(r, r);
                md = min(md, d);
            }
        }
    }
    return sqrt(md);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::ShaderNode::*;

    #[test]
    fn test_compile_constant_pbr() {
        let tree = MaterialOutput {
            surface: Box::new(PrincipledBsdf {
                base_color: Box::new(ConstVec3([0.8, 0.2, 0.1])),
                metallic: Box::new(ConstFloat(0.0)),
                roughness: Box::new(ConstFloat(0.5)),
                normal: None,
                emission: Box::new(ConstVec3([0.0, 0.0, 0.0])),
                emission_strength: Box::new(ConstFloat(0.0)),
                ao: None,
                alpha: Box::new(ConstFloat(1.0)),
                subsurface: None,
                subsurface_color: None,
                clearcoat: None,
                clearcoat_roughness: None,
                anisotropic: None,
                anisotropic_rotation: None,
                sheen: None,
                sheen_tint: None,
                transmission: None,
                ior: None,
            }),
        };

        let mat = compile(&tree);
        assert!(mat.fragment_wgsl.contains("pbr_shade"));
        assert!(mat.fragment_wgsl.contains("D_GGX"));
        assert!(mat.fragment_wgsl.contains("F_Schlick"));
        assert!(mat.vertex_wgsl.contains("vs_main"));
        println!("Fragment WGSL ({} bytes):\n{}", mat.fragment_wgsl.len(), &mat.fragment_wgsl[..500.min(mat.fragment_wgsl.len())]);
    }

    #[test]
    fn test_compile_noise_material() {
        let tree = MaterialOutput {
            surface: Box::new(PrincipledBsdf {
                base_color: Box::new(NoiseTexture {
                    scale: Box::new(ConstFloat(5.0)),
                    detail: Box::new(ConstFloat(2.0)),
                    roughness: Box::new(ConstFloat(0.5)),
                }),
                metallic: Box::new(ConstFloat(0.8)),
                roughness: Box::new(ConstFloat(0.2)),
                normal: None,
                emission: Box::new(ConstVec3([0.0, 0.0, 0.0])),
                emission_strength: Box::new(ConstFloat(0.0)),
                ao: None,
                alpha: Box::new(ConstFloat(1.0)),
                subsurface: None,
                subsurface_color: None,
                clearcoat: None,
                clearcoat_roughness: None,
                anisotropic: None,
                anisotropic_rotation: None,
                sheen: None,
                sheen_tint: None,
                transmission: None,
                ior: None,
            }),
        };

        let mat = compile(&tree);
        assert!(mat.fragment_wgsl.contains("fbm_noise"));
        println!("Noise material WGSL ({} bytes)", mat.fragment_wgsl.len());
    }
}
