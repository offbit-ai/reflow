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
        ShaderNode::ConstFloat { c: v } => format!("{:.6}", v),
        ShaderNode::ConstVec3 { c: v } => format!("vec3f({:.6}, {:.6}, {:.6})", v[0], v[1], v[2]),
        ShaderNode::ConstVec4 { c: v } => {
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
                (MathOpType::Smoothstep, Some(_)) => format!("smoothstep(0.0, 1.0, {va})"),
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
                    format!(
                        "mix({va}, mix(2.0*{va}*{vb}, 1.0-2.0*(1.0-{va})*(1.0-{vb}), step(0.5,{va})), {vf})"
                    )
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
                let c = stops
                    .first()
                    .map(|s| s.color)
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]);
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
        ShaderNode::Clamp {
            input,
            min_val,
            max_val,
        } => {
            let vi = emit_node(ctx, input);
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var} = clamp({vi}, {:.6}, {:.6});",
                min_val, max_val
            ));
            var
        }
        ShaderNode::MapRange {
            input,
            from_min,
            from_max,
            to_min,
            to_max,
        } => {
            let vi = emit_node(ctx, input);
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var} = {:.6} + ({vi} - {:.6}) / {:.6} * {:.6};",
                to_min,
                from_min,
                (from_max - from_min).max(0.0001),
                to_max - to_min
            ));
            var
        }

        // ═══ Procedural textures ═══
        ShaderNode::NoiseTexture {
            scale,
            detail: _,
            roughness: _,
        } => {
            let vs = emit_node(ctx, scale);
            ctx.needs_uv = true;
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var} = vec3f(fbm_noise(in.world_pos * {vs}));"
            ));
            var
        }
        ShaderNode::CheckerTexture {
            scale,
            color1,
            color2,
        } => {
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
            ctx.emit(&format!(
                "let {var} = vec3f(voronoi_noise(in.world_pos * {vs}));"
            ));
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

        // ═══ Principled BSDF — multi-light PBR shading with advanced lobes ═══
        ShaderNode::PrincipledBsdf {
            base_color,
            metallic,
            roughness,
            normal,
            emission,
            emission_strength,
            ao,
            alpha,
            clearcoat,
            clearcoat_roughness,
            subsurface,
            subsurface_color,
            sheen,
            sheen_tint,
            anisotropic: _,
            anisotropic_rotation: _,
            transmission: _,
            ior: _,
        } => {
            let vbc = emit_node(ctx, base_color);
            let vm = emit_node(ctx, metallic);
            let vr = emit_node(ctx, roughness);
            let ve = emit_node(ctx, emission);
            let ves = emit_node(ctx, emission_strength);
            let va = ao
                .as_ref()
                .map(|n| emit_node(ctx, n))
                .unwrap_or_else(|| "1.0".to_string());
            let _valpha = emit_node(ctx, alpha);
            let vn = normal
                .as_ref()
                .map(|n| emit_node(ctx, n))
                .unwrap_or_else(|| "normalize(in.world_normal)".to_string());

            // Advanced lobes
            let vcc = clearcoat
                .as_ref()
                .map(|n| emit_node(ctx, n))
                .unwrap_or_else(|| "0.0".to_string());
            let vccr = clearcoat_roughness
                .as_ref()
                .map(|n| emit_node(ctx, n))
                .unwrap_or_else(|| "0.03".to_string());
            let vsss = subsurface
                .as_ref()
                .map(|n| emit_node(ctx, n))
                .unwrap_or_else(|| "0.0".to_string());
            let vsss_c = subsurface_color
                .as_ref()
                .map(|n| emit_node(ctx, n))
                .unwrap_or_else(|| vbc.clone());
            let vsh = sheen
                .as_ref()
                .map(|n| emit_node(ctx, n))
                .unwrap_or_else(|| "0.0".to_string());
            let vsh_t = sheen_tint
                .as_ref()
                .map(|n| emit_node(ctx, n))
                .unwrap_or_else(|| "0.5".to_string());

            ctx.needs_camera_pos = true;
            let var = ctx.fresh_var();

            // Base PBR
            ctx.emit(&format!(
                "var {var} = pbr_shade_multi({vbc}, {vm}, {vr}, {vn}, normalize(u_scene.camera_pos - in.world_pos), in.world_pos, {va}, {ve} * {ves});"
            ));

            // Clearcoat: secondary specular lobe with low roughness
            ctx.emit(&format!(
                "{{ let cc_V = normalize(u_scene.camera_pos - in.world_pos); let cc_R = reflect(-cc_V, {vn}); let cc_NoV = max(dot({vn}, cc_V), 0.001); let cc_fresnel = 0.04 + 0.96 * pow(1.0 - cc_NoV, 5.0); {var} += vec3f(cc_fresnel * {vcc} * (1.0 - {vccr})); }}"
            ));

            // Subsurface scattering approximation (wrap lighting)
            ctx.emit(&format!(
                "{{ let sss_wrap = 0.5; let sss_NdotL = (dot({vn}, normalize(u_scene.light_dir)) + sss_wrap) / (1.0 + sss_wrap); {var} = mix({var}, {vsss_c} * max(sss_NdotL, 0.0), {vsss}); }}"
            ));

            // Sheen (fabric/velvet — Fresnel-based edge glow)
            ctx.emit(&format!(
                "{{ let sh_V = normalize(u_scene.camera_pos - in.world_pos); let sh_NdotV = max(dot({vn}, sh_V), 0.0); let sh_factor = pow(1.0 - sh_NdotV, 3.0); let sh_color = mix(vec3f(1.0), {vbc}, {vsh_t}); {var} += sh_color * sh_factor * {vsh}; }}"
            ));

            var
        }

        // ═══ Material output ═══
        ShaderNode::MaterialOutput { surface } => emit_node(ctx, surface),

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

    // Scene + light + shadow uniforms
    frag.push_str(UNIFORM_STRUCTS);
    frag.push_str(SHADOW_UNIFORMS);

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

/// Compile a ShaderNode tree into a `fn shade(ro, rd, t) -> vec3f` WGSL function
/// compatible with the SDF ray march renderer. The SDF renderer provides:
/// - `ro`: ray origin (camera), `rd`: ray direction, `t`: hit distance
/// - `calc_normal(p)`: SDF gradient normal
/// - `sdf_scene(p)`: scene distance function
/// - `LIGHT_DIR`, `LIGHT_COLOR`, `AMBIENT`: scene lighting constants
/// - `soft_shadow(ro, rd, mint, maxt)`: if soft shadows enabled
/// - `calc_ao(pos, nor)`: if AO enabled
///
/// Compile a ShaderNode tree into a `fn shade(ro, rd, t) -> vec3f` WGSL string
/// for the SDF ray march renderer. Extracts material parameters from the IR
/// and generates a shade function using SDF-context variables.
pub fn compile_sdf_shade(root: &ShaderNode) -> String {
    compile_sdf_shade_named(root, "shade")
}

pub fn compile_sdf_shade_named(root: &ShaderNode, fn_name: &str) -> String {
    let mut ctx = SdfShadeCtx::default();
    let params = extract_sdf_material(&mut ctx, root);

    let mut shade = String::new();
    shade.push_str(PBR_SDF_FUNCTIONS);
    shade.push_str(SDF_SHADE_NOISE_FUNCTIONS);

    let probe_fn_name = format!("{fn_name}_probe");
    shade.push_str(&build_sdf_shade_function(
        &probe_fn_name,
        &ctx.code,
        &params,
        true,
    ));
    shade.push_str(&build_sdf_shade_function(
        fn_name, &ctx.code, &params, false,
    ));
    shade
}

fn build_sdf_shade_function(
    fn_name: &str,
    setup_code: &str,
    params: &SdfMaterialExprs,
    is_probe: bool,
) -> String {
    let mut shade = String::new();
    shade.push_str(&format!(
        "fn {fn_name}(ro: vec3f, rd: vec3f, t: f32) -> vec3f {{\n"
    ));
    shade.push_str("    let p = ro + rd * t;\n");
    shade.push_str("    let N_geom = calc_normal(p);\n");
    shade.push_str("    let V = -rd;\n");
    shade.push_str(setup_code);
    shade.push('\n');

    shade.push_str(&format!(
        "    let base_color = clamp({}, vec3f(0.0), vec3f(1.0));\n",
        params.base_color
    ));
    shade.push_str(&format!(
        "    let metallic = clamp({}, 0.0, 1.0);\n",
        params.metallic
    ));
    shade.push_str(&format!(
        "    let roughness = clamp({}, 0.02, 1.0);\n",
        params.roughness
    ));
    shade.push_str(&format!(
        "    let emission = {} * {};\n",
        params.emission, params.emission_strength
    ));
    shade.push_str(&format!(
        "    let alpha = clamp({}, 0.0, 1.0);\n",
        params.alpha
    ));
    shade.push_str(&format!(
        "    let transmission = clamp({}, 0.0, 1.0);\n",
        params
            .transmission
            .clone()
            .unwrap_or_else(|| "(1.0 - alpha)".to_string())
    ));
    shade.push_str(&format!("    let ior = max({}, 1.01);\n", params.ior));
    shade.push_str(&format!(
        "    let ao = clamp(calc_ao(p, N_geom) * {}, 0.0, 1.0);\n",
        params.ao.clone().unwrap_or_else(|| "1.0".to_string())
    ));
    if let Some(normal) = &params.normal {
        shade.push_str(&format!("    let N = normalize({});\n", normal));
    } else {
        shade.push_str("    let N = N_geom;\n");
    }
    shade.push('\n');

    shade.push_str("    let NoV = max(dot(N, V), 0.0);\n");
    shade.push_str("    let dielectric_f0 = pow((ior - 1.0) / (ior + 1.0), 2.0);\n");
    shade.push_str("    let F0 = mix(vec3f(dielectric_f0), base_color, metallic);\n");
    shade.push_str("    let fresnel = F_Schlick_sdf(NoV, F0);\n");
    shade.push_str(
        "    let fresnel_strength = clamp(max(max(fresnel.x, fresnel.y), fresnel.z), 0.0, 1.0);\n",
    );
    shade.push_str("    let thickness = sdf_refract_thickness(p, rd, N, ior);\n");
    shade.push_str("    let density = 1.0 - exp(-thickness * mix(0.015, 0.0012, transmission));\n");
    shade.push_str("    let thin_factor = smoothstep(0.03, 0.24, thickness);\n");
    if is_probe {
        shade.push_str(
            "    let refracted = sdf_refract_color_env(p, rd, N, ior, base_color, transmission, thickness);\n",
        );
    } else {
        shade.push_str(
            "    let refracted = sdf_refract_color_scene(p, rd, N, ior, base_color, transmission, thickness);\n",
        );
    }
    shade.push_str(
        "    let transmitted_light = sdf_transmission_light(p, rd, N, ior, roughness, transmission, thickness, base_color);\n",
    );
    // Opaque PBR surface (full base_color for non-transparent materials)
    shade.push_str(
        "    let opaque_surface = pbr_shade_sdf(base_color, metallic, roughness, N, V, ao, emission);\n",
    );
    // Glass interior: density-dependent body tint for internal scattering
    shade.push_str(
        "    let glass_body = mix(base_color * 0.001, base_color * 0.012, clamp(density * 0.5, 0.0, 1.0));\n",
    );
    shade.push_str(
        "    let glass_interior = pbr_shade_sdf(glass_body * mix(0.08, 0.82, roughness), metallic, roughness, N, V, ao, emission);\n",
    );

    // Reflection sampling
    if is_probe {
        shade.push_str("    let reflected = sdf_env_color(reflect(rd, N));\n");
    } else {
        shade.push_str(
            "    let reflected = sdf_scene_reflection_color(p, rd, N, roughness, transmission);\n",
        );
    }

    // Fresnel: Schlick + edge boost for visible rim on transparent surfaces
    shade.push_str("    let edge_boost = transmission * pow(1.0 - NoV, 3.0) * 0.25;\n");
    shade.push_str("    let F = clamp(fresnel_strength + edge_boost, 0.0, 1.0);\n");

    // Glass path: refraction dominates; scattering only in dense/thick regions
    shade.push_str("    let scatter_factor = clamp(density * 2.0, 0.0, 0.35);\n");
    shade.push_str(
        "    let glass_term = refracted + transmitted_light * scatter_factor + glass_interior * clamp(density * thin_factor * 0.3, 0.0, 0.12);\n",
    );

    shade.push_str("    let effective_T = transmission;\n");
    // Physically-based compositing: opaque↔glass by transmission, then Fresnel reflection
    shade.push_str("    let non_reflected = mix(opaque_surface, glass_term, effective_T);\n");
    shade.push_str("    var col = mix(non_reflected, reflected, F);\n");
    // Surface edge darkening: cube edges where normal transitions between faces
    shade.push_str("    let entry_abs = abs(N_geom);\n");
    shade.push_str("    let entry_max_n = max(max(entry_abs.x, entry_abs.y), entry_abs.z);\n");
    shade.push_str("    let entry_mid_n = entry_abs.x + entry_abs.y + entry_abs.z - entry_max_n - min(min(entry_abs.x, entry_abs.y), entry_abs.z);\n");
    // Inner shadow: thin dark lines at geometry edges (not broad gradients)
    shade.push_str("    let edge_ratio = entry_mid_n / max(entry_max_n, 0.001);\n");
    shade.push_str("    let inner_shadow = exp(-pow((edge_ratio - 0.45) / 0.12, 2.0)) * 0.65;\n");
    shade.push_str("    col *= 1.0 - inner_shadow * transmission;\n");
    shade.push_str("    col *= mix(0.9, 1.0, ao);\n");
    shade.push_str("    return col;\n");
    shade.push_str("}\n\n");
    shade
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SdfExprType {
    Float,
    Vec3,
}

struct SdfExpr {
    expr: String,
    ty: SdfExprType,
}

#[derive(Default)]
struct SdfShadeCtx {
    code: String,
    var_counter: usize,
    needs_noise: bool,
}

impl SdfShadeCtx {
    fn emit(&mut self, line: &str) {
        self.code.push_str("    ");
        self.code.push_str(line);
        self.code.push('\n');
    }

    fn fresh_var(&mut self) -> String {
        let var = format!("sdf_v{}", self.var_counter);
        self.var_counter += 1;
        var
    }
}

struct SdfMaterialExprs {
    base_color: String,
    metallic: String,
    roughness: String,
    normal: Option<String>,
    emission: String,
    emission_strength: String,
    ao: Option<String>,
    alpha: String,
    transmission: Option<String>,
    ior: String,
}

fn extract_sdf_material(ctx: &mut SdfShadeCtx, root: &ShaderNode) -> SdfMaterialExprs {
    match root {
        ShaderNode::MaterialOutput { surface } => extract_sdf_material(ctx, surface),
        ShaderNode::PrincipledBsdf {
            base_color,
            metallic,
            roughness,
            normal,
            emission,
            emission_strength,
            ao,
            alpha,
            transmission,
            ior,
            ..
        } => SdfMaterialExprs {
            base_color: sdf_to_vec3(emit_sdf_node(ctx, base_color)),
            metallic: sdf_to_float(emit_sdf_node(ctx, metallic)),
            roughness: sdf_to_float(emit_sdf_node(ctx, roughness)),
            normal: normal
                .as_deref()
                .map(|node| sdf_to_vec3(emit_sdf_node(ctx, node))),
            emission: sdf_to_vec3(emit_sdf_node(ctx, emission)),
            emission_strength: sdf_to_float(emit_sdf_node(ctx, emission_strength)),
            ao: ao
                .as_deref()
                .map(|node| sdf_to_float(emit_sdf_node(ctx, node))),
            alpha: sdf_to_float(emit_sdf_node(ctx, alpha)),
            transmission: transmission
                .as_deref()
                .map(|node| sdf_to_float(emit_sdf_node(ctx, node))),
            ior: ior
                .as_deref()
                .map(|node| sdf_to_float(emit_sdf_node(ctx, node)))
                .unwrap_or_else(|| "1.31".to_string()),
        },
        _ => SdfMaterialExprs {
            base_color: "vec3f(0.8, 0.8, 0.8)".to_string(),
            metallic: "0.0".to_string(),
            roughness: "0.5".to_string(),
            normal: None,
            emission: "vec3f(0.0)".to_string(),
            emission_strength: "0.0".to_string(),
            ao: None,
            alpha: "1.0".to_string(),
            transmission: None,
            ior: "1.31".to_string(),
        },
    }
}

#[derive(Clone)]
struct SdfEvalEnv {
    position: String,
    geometric_normal: String,
    view: String,
}

impl Default for SdfEvalEnv {
    fn default() -> Self {
        Self {
            position: "p".to_string(),
            geometric_normal: "N_geom".to_string(),
            view: "V".to_string(),
        }
    }
}

impl SdfEvalEnv {
    fn sampled(position: String, view: impl Into<String>) -> Self {
        let geometric_normal = format!("calc_normal({position})");
        Self {
            position,
            geometric_normal,
            view: view.into(),
        }
    }
}

fn emit_sdf_node(ctx: &mut SdfShadeCtx, node: &ShaderNode) -> SdfExpr {
    emit_sdf_node_in_env(ctx, node, &SdfEvalEnv::default())
}

fn emit_sdf_node_in_env(ctx: &mut SdfShadeCtx, node: &ShaderNode, env: &SdfEvalEnv) -> SdfExpr {
    match node {
        ShaderNode::MaterialOutput { surface } => emit_sdf_node_in_env(ctx, surface, env),
        ShaderNode::ConstFloat { c } => SdfExpr {
            expr: format!("{:.6}", c),
            ty: SdfExprType::Float,
        },
        ShaderNode::ConstVec3 { c } => SdfExpr {
            expr: format!("vec3f({:.6}, {:.6}, {:.6})", c[0], c[1], c[2]),
            ty: SdfExprType::Vec3,
        },
        ShaderNode::ConstVec4 { c } => SdfExpr {
            expr: format!("vec3f({:.6}, {:.6}, {:.6})", c[0], c[1], c[2]),
            ty: SdfExprType::Vec3,
        },
        ShaderNode::TexCoord => SdfExpr {
            expr: format!("vec3f({}.x, {}.z, 0.0)", env.position, env.position),
            ty: SdfExprType::Vec3,
        },
        ShaderNode::ObjectPosition => SdfExpr {
            expr: env.position.clone(),
            ty: SdfExprType::Vec3,
        },
        ShaderNode::ObjectNormal => SdfExpr {
            expr: env.geometric_normal.clone(),
            ty: SdfExprType::Vec3,
        },
        ShaderNode::CameraVector => SdfExpr {
            expr: env.view.clone(),
            ty: SdfExprType::Vec3,
        },
        ShaderNode::Time => SdfExpr {
            expr: "u.time".to_string(),
            ty: SdfExprType::Float,
        },
        ShaderNode::MathOp { op, a, b } => emit_sdf_math(ctx, env, op, a, b.as_deref()),
        ShaderNode::ColorMix { mode, fac, a, b } => emit_sdf_color_mix(ctx, env, mode, fac, a, b),
        ShaderNode::ColorRamp { stops, input } => emit_sdf_color_ramp(ctx, env, stops, input),
        ShaderNode::SeparateXYZ { input, component } => {
            let input = sdf_to_vec3(emit_sdf_node_in_env(ctx, input, env));
            let channel = match component.as_str() {
                "y" => "y",
                "z" => "z",
                _ => "x",
            };
            SdfExpr {
                expr: format!("{input}.{channel}"),
                ty: SdfExprType::Float,
            }
        }
        ShaderNode::CombineXYZ { x, y, z } => {
            let x = sdf_to_float(emit_sdf_node_in_env(ctx, x, env));
            let y = sdf_to_float(emit_sdf_node_in_env(ctx, y, env));
            let z = sdf_to_float(emit_sdf_node_in_env(ctx, z, env));
            let var = ctx.fresh_var();
            ctx.emit(&format!("let {var} = vec3f({x}, {y}, {z});"));
            SdfExpr {
                expr: var,
                ty: SdfExprType::Vec3,
            }
        }
        ShaderNode::Fresnel { ior } => {
            let ior = sdf_to_float(emit_sdf_node_in_env(ctx, ior, env));
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var}_f0 = pow((max({ior}, 1.01) - 1.0) / (max({ior}, 1.01) + 1.0), 2.0);"
            ));
            ctx.emit(&format!(
                "let {var} = {var}_f0 + (1.0 - {var}_f0) * pow(1.0 - max(dot(normalize({}), normalize({})), 0.0), 5.0);",
                env.geometric_normal, env.view
            ));
            SdfExpr {
                expr: var,
                ty: SdfExprType::Float,
            }
        }
        ShaderNode::NormalMap { strength, color } => emit_sdf_normal_map(ctx, env, strength, color),
        ShaderNode::BumpMap { strength, height } => emit_sdf_bump_map(ctx, env, strength, height),
        ShaderNode::Clamp {
            input,
            min_val,
            max_val,
        } => {
            let input_expr = emit_sdf_node_in_env(ctx, input, env);
            let value = match input_expr.ty {
                SdfExprType::Float => {
                    format!("clamp({}, {:.6}, {:.6})", input_expr.expr, min_val, max_val)
                }
                SdfExprType::Vec3 => format!(
                    "clamp({}, vec3f({:.6}), vec3f({:.6}))",
                    input_expr.expr, min_val, max_val
                ),
            };
            let var = ctx.fresh_var();
            ctx.emit(&format!("let {var} = {value};"));
            SdfExpr {
                expr: var,
                ty: input_expr.ty,
            }
        }
        ShaderNode::MapRange {
            input,
            from_min,
            from_max,
            to_min,
            to_max,
        } => {
            let input_expr = emit_sdf_node_in_env(ctx, input, env);
            let value = match input_expr.ty {
                SdfExprType::Float => format!(
                    "{to_min:.6} + ({expr} - {from_min:.6}) / {range:.6} * {to_range:.6}",
                    expr = input_expr.expr,
                    range = (from_max - from_min).max(0.0001),
                    to_range = to_max - to_min
                ),
                SdfExprType::Vec3 => format!(
                    "vec3f({to_min:.6}) + ({expr} - vec3f({from_min:.6})) / vec3f({range:.6}) * vec3f({to_range:.6})",
                    expr = input_expr.expr,
                    range = (from_max - from_min).max(0.0001),
                    to_range = to_max - to_min
                ),
            };
            let var = ctx.fresh_var();
            ctx.emit(&format!("let {var} = {value};"));
            SdfExpr {
                expr: var,
                ty: input_expr.ty,
            }
        }
        ShaderNode::NoiseTexture { scale, .. } => {
            ctx.needs_noise = true;
            let scale = sdf_to_float(emit_sdf_node_in_env(ctx, scale, env));
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var} = vec3f(shade_fbm_noise({} * {scale}));",
                env.position
            ));
            SdfExpr {
                expr: var,
                ty: SdfExprType::Vec3,
            }
        }
        ShaderNode::CheckerTexture {
            scale,
            color1,
            color2,
        } => {
            let scale = sdf_to_float(emit_sdf_node_in_env(ctx, scale, env));
            let color1 = sdf_to_vec3(emit_sdf_node_in_env(ctx, color1, env));
            let color2 = sdf_to_vec3(emit_sdf_node_in_env(ctx, color2, env));
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var} = select({color1}, {color2}, (floor({p}.x * {scale}) + floor({p}.y * {scale}) + floor({p}.z * {scale})) % 2.0 < 1.0);",
                p = env.position
            ));
            SdfExpr {
                expr: var,
                ty: SdfExprType::Vec3,
            }
        }
        ShaderNode::VoronoiTexture { scale, .. } => {
            ctx.needs_noise = true;
            let scale = sdf_to_float(emit_sdf_node_in_env(ctx, scale, env));
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var} = vec3f(shade_voronoi_noise({} * {scale}));",
                env.position
            ));
            SdfExpr {
                expr: var,
                ty: SdfExprType::Vec3,
            }
        }
        ShaderNode::Mapping {
            location,
            rotation,
            scale,
            input,
        } => {
            let input = sdf_to_vec3(emit_sdf_node_in_env(ctx, input, env));
            let var = ctx.fresh_var();
            ctx.emit(&format!(
                "let {var} = rot_xyz(({input} + vec3f({:.6}, {:.6}, {:.6})) * vec3f({:.6}, {:.6}, {:.6}), vec3f({:.6}, {:.6}, {:.6}));",
                location[0],
                location[1],
                location[2],
                scale[0],
                scale[1],
                scale[2],
                rotation[0],
                rotation[1],
                rotation[2]
            ));
            SdfExpr {
                expr: var,
                ty: SdfExprType::Vec3,
            }
        }
        _ => SdfExpr {
            expr: "vec3f(0.8, 0.8, 0.8)".to_string(),
            ty: SdfExprType::Vec3,
        },
    }
}

fn emit_sdf_normal_map(
    ctx: &mut SdfShadeCtx,
    env: &SdfEvalEnv,
    strength: &ShaderNode,
    color: &ShaderNode,
) -> SdfExpr {
    let strength = sdf_to_float(emit_sdf_node_in_env(ctx, strength, env));
    let color = sdf_to_vec3(emit_sdf_node_in_env(ctx, color, env));
    let var = ctx.fresh_var();
    ctx.emit(&format!(
        "let {var}_base_n = normalize({});",
        env.geometric_normal
    ));
    ctx.emit(&format!(
        "let {var}_up = select(vec3f(0.0, 1.0, 0.0), vec3f(1.0, 0.0, 0.0), abs({var}_base_n.y) > 0.98);"
    ));
    ctx.emit(&format!(
        "let {var}_tangent = normalize(cross({var}_up, {var}_base_n));"
    ));
    ctx.emit(&format!(
        "let {var}_bitangent = cross({var}_base_n, {var}_tangent);"
    ));
    ctx.emit(&format!(
        "let {var}_raw = clamp({color}, vec3f(0.0), vec3f(1.0)) * 2.0 - vec3f(1.0);"
    ));
    ctx.emit(&format!(
        "let {var}_mapped = normalize({var}_tangent * {var}_raw.x + {var}_bitangent * {var}_raw.y + {var}_base_n * max({var}_raw.z, 0.0001));"
    ));
    ctx.emit(&format!(
        "let {var} = normalize(mix({var}_base_n, {var}_mapped, clamp({strength}, 0.0, 1.0)));"
    ));
    SdfExpr {
        expr: var,
        ty: SdfExprType::Vec3,
    }
}

fn emit_sdf_bump_map(
    ctx: &mut SdfShadeCtx,
    env: &SdfEvalEnv,
    strength: &ShaderNode,
    height: &ShaderNode,
) -> SdfExpr {
    let strength = sdf_to_float(emit_sdf_node_in_env(ctx, strength, env));
    let var = ctx.fresh_var();
    ctx.emit(&format!(
        "let {var}_base_n = normalize({});",
        env.geometric_normal
    ));
    ctx.emit(&format!(
        "let {var}_up = select(vec3f(0.0, 1.0, 0.0), vec3f(1.0, 0.0, 0.0), abs({var}_base_n.y) > 0.98);"
    ));
    ctx.emit(&format!(
        "let {var}_tangent = normalize(cross({var}_up, {var}_base_n));"
    ));
    ctx.emit(&format!(
        "let {var}_bitangent = cross({var}_base_n, {var}_tangent);"
    ));
    ctx.emit(&format!("let {var}_eps = max(EPSILON * 8.0, 0.01);"));
    ctx.emit(&format!(
        "let {var}_tan_pos = ({}) + {var}_tangent * {var}_eps;",
        env.position
    ));
    ctx.emit(&format!(
        "let {var}_tan_neg = ({}) - {var}_tangent * {var}_eps;",
        env.position
    ));
    ctx.emit(&format!(
        "let {var}_bitan_pos = ({}) + {var}_bitangent * {var}_eps;",
        env.position
    ));
    ctx.emit(&format!(
        "let {var}_bitan_neg = ({}) - {var}_bitangent * {var}_eps;",
        env.position
    ));

    let tan_pos = SdfEvalEnv::sampled(format!("{var}_tan_pos"), env.view.clone());
    let tan_neg = SdfEvalEnv::sampled(format!("{var}_tan_neg"), env.view.clone());
    let bitan_pos = SdfEvalEnv::sampled(format!("{var}_bitan_pos"), env.view.clone());
    let bitan_neg = SdfEvalEnv::sampled(format!("{var}_bitan_neg"), env.view.clone());

    let h_tan_pos = sdf_to_float(emit_sdf_node_in_env(ctx, height, &tan_pos));
    let h_tan_neg = sdf_to_float(emit_sdf_node_in_env(ctx, height, &tan_neg));
    let h_bitan_pos = sdf_to_float(emit_sdf_node_in_env(ctx, height, &bitan_pos));
    let h_bitan_neg = sdf_to_float(emit_sdf_node_in_env(ctx, height, &bitan_neg));

    ctx.emit(&format!(
        "let {var}_dhdx = ({h_tan_pos} - {h_tan_neg}) / ({var}_eps * 2.0);"
    ));
    ctx.emit(&format!(
        "let {var}_dhdy = ({h_bitan_pos} - {h_bitan_neg}) / ({var}_eps * 2.0);"
    ));
    ctx.emit(&format!(
        "let {var} = normalize({var}_base_n - ({var}_tangent * {var}_dhdx + {var}_bitangent * {var}_dhdy) * clamp({strength}, 0.0, 1.0));"
    ));
    SdfExpr {
        expr: var,
        ty: SdfExprType::Vec3,
    }
}

fn emit_sdf_math(
    ctx: &mut SdfShadeCtx,
    env: &SdfEvalEnv,
    op: &MathOpType,
    a: &ShaderNode,
    b: Option<&ShaderNode>,
) -> SdfExpr {
    let a_expr = emit_sdf_node_in_env(ctx, a, env);
    let b_expr = b.map(|node| emit_sdf_node_in_env(ctx, node, env));
    let result_ty = match op {
        MathOpType::Dot | MathOpType::Distance | MathOpType::Length => SdfExprType::Float,
        MathOpType::Cross | MathOpType::Normalize | MathOpType::Reflect => SdfExprType::Vec3,
        _ => b_expr
            .as_ref()
            .map(|rhs| sdf_result_type(a_expr.ty, rhs.ty))
            .unwrap_or(a_expr.ty),
    };

    let lhs = sdf_coerce(&a_expr, result_ty);
    let rhs = b_expr
        .as_ref()
        .map(|expr| sdf_coerce(expr, result_ty))
        .unwrap_or_default();

    let value = match op {
        MathOpType::Add => format!("({lhs} + {rhs})"),
        MathOpType::Subtract => format!("({lhs} - {rhs})"),
        MathOpType::Multiply => format!("({lhs} * {rhs})"),
        MathOpType::Divide => format!("({lhs} / {rhs})"),
        MathOpType::Power => format!("pow({lhs}, {rhs})"),
        MathOpType::Min => format!("min({lhs}, {rhs})"),
        MathOpType::Max => format!("max({lhs}, {rhs})"),
        MathOpType::Modulo => format!("({lhs} % {rhs})"),
        MathOpType::Atan2 => format!("atan2({lhs}, {rhs})"),
        MathOpType::Smoothstep => format!("smoothstep(0.0, 1.0, {lhs})"),
        MathOpType::Lerp => format!("mix({lhs}, {rhs}, 0.5)"),
        MathOpType::Step => format!("step({lhs}, {rhs})"),
        MathOpType::Dot => {
            let lhs = sdf_to_vec3(a_expr);
            let rhs = b_expr
                .map(sdf_to_vec3)
                .unwrap_or_else(|| "vec3f(0.0)".to_string());
            format!("dot({lhs}, {rhs})")
        }
        MathOpType::Cross => {
            let lhs = sdf_to_vec3(a_expr);
            let rhs = b_expr
                .map(sdf_to_vec3)
                .unwrap_or_else(|| "vec3f(0.0)".to_string());
            format!("cross({lhs}, {rhs})")
        }
        MathOpType::Distance => {
            let lhs = sdf_to_vec3(a_expr);
            let rhs = b_expr
                .map(sdf_to_vec3)
                .unwrap_or_else(|| "vec3f(0.0)".to_string());
            format!("distance({lhs}, {rhs})")
        }
        MathOpType::Reflect => {
            let lhs = sdf_to_vec3(a_expr);
            let rhs = b_expr
                .map(sdf_to_vec3)
                .unwrap_or_else(|| env.geometric_normal.clone());
            format!("reflect({lhs}, {rhs})")
        }
        MathOpType::Sqrt => format!("sqrt(abs({lhs}))"),
        MathOpType::Abs => format!("abs({lhs})"),
        MathOpType::Sin => format!("sin({lhs})"),
        MathOpType::Cos => format!("cos({lhs})"),
        MathOpType::Tan => format!("tan({lhs})"),
        MathOpType::Asin => format!("asin(clamp({lhs}, -1.0, 1.0))"),
        MathOpType::Acos => format!("acos(clamp({lhs}, -1.0, 1.0))"),
        MathOpType::Floor => format!("floor({lhs})"),
        MathOpType::Ceil => format!("ceil({lhs})"),
        MathOpType::Fract => format!("fract({lhs})"),
        MathOpType::Sign => format!("sign({lhs})"),
        MathOpType::Log => format!("log({lhs})"),
        MathOpType::Exp => format!("exp({lhs})"),
        MathOpType::Normalize => {
            let lhs = sdf_to_vec3(a_expr);
            format!("normalize({lhs})")
        }
        MathOpType::Length => {
            let lhs = sdf_to_vec3(a_expr);
            format!("length({lhs})")
        }
        MathOpType::Negate => format!("(-{lhs})"),
        MathOpType::Invert => format!("(1.0 - {lhs})"),
    };

    let var = ctx.fresh_var();
    ctx.emit(&format!("let {var} = {value};"));
    SdfExpr {
        expr: var,
        ty: result_ty,
    }
}

fn emit_sdf_color_mix(
    ctx: &mut SdfShadeCtx,
    env: &SdfEvalEnv,
    mode: &MixMode,
    fac: &ShaderNode,
    a: &ShaderNode,
    b: &ShaderNode,
) -> SdfExpr {
    let fac_expr = emit_sdf_node_in_env(ctx, fac, env);
    let a_expr = emit_sdf_node_in_env(ctx, a, env);
    let b_expr = emit_sdf_node_in_env(ctx, b, env);
    let result_ty = sdf_result_type(a_expr.ty, b_expr.ty);
    let lhs = sdf_coerce(&a_expr, result_ty);
    let rhs = sdf_coerce(&b_expr, result_ty);
    let fac = if result_ty == SdfExprType::Float {
        sdf_to_float(fac_expr)
    } else {
        sdf_coerce(&fac_expr, SdfExprType::Vec3)
    };

    let value = match mode {
        MixMode::Mix => format!("mix({lhs}, {rhs}, {fac})"),
        MixMode::Add => format!("({lhs} + {rhs} * {fac})"),
        MixMode::Multiply => format!("mix({lhs}, {lhs} * {rhs}, {fac})"),
        MixMode::Screen => format!("mix({lhs}, 1.0 - (1.0 - {lhs}) * (1.0 - {rhs}), {fac})"),
        MixMode::Overlay => format!(
            "mix({lhs}, mix(2.0 * {lhs} * {rhs}, 1.0 - 2.0 * (1.0 - {lhs}) * (1.0 - {rhs}), step(0.5, {lhs})), {fac})"
        ),
        MixMode::Darken => format!("mix({lhs}, min({lhs}, {rhs}), {fac})"),
        MixMode::Lighten => format!("mix({lhs}, max({lhs}, {rhs}), {fac})"),
        MixMode::Difference => format!("mix({lhs}, abs({lhs} - {rhs}), {fac})"),
        MixMode::Subtract => format!("mix({lhs}, {lhs} - {rhs}, {fac})"),
        _ => format!("mix({lhs}, {rhs}, {fac})"),
    };

    let var = ctx.fresh_var();
    ctx.emit(&format!("let {var} = {value};"));
    SdfExpr {
        expr: var,
        ty: result_ty,
    }
}

fn emit_sdf_color_ramp(
    ctx: &mut SdfShadeCtx,
    env: &SdfEvalEnv,
    stops: &[ColorStop],
    input: &ShaderNode,
) -> SdfExpr {
    let input = sdf_to_float(emit_sdf_node_in_env(ctx, input, env));
    let var = ctx.fresh_var();
    if stops.len() < 2 {
        let color = stops
            .first()
            .map(|stop| stop.color)
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        ctx.emit(&format!(
            "let {var} = vec3f({:.6}, {:.6}, {:.6});",
            color[0], color[1], color[2]
        ));
    } else {
        ctx.emit(&format!("var {var} = vec3f(0.0);"));
        for pair in stops.windows(2) {
            let a = &pair[0];
            let b = &pair[1];
            ctx.emit(&format!(
                "if ({input} >= {:.6} && {input} <= {:.6}) {{ let t = ({input} - {:.6}) / {:.6}; {var} = mix(vec3f({:.6}, {:.6}, {:.6}), vec3f({:.6}, {:.6}, {:.6}), t); }}",
                a.position,
                b.position,
                a.position,
                (b.position - a.position).max(0.0001),
                a.color[0],
                a.color[1],
                a.color[2],
                b.color[0],
                b.color[1],
                b.color[2]
            ));
        }
    }

    SdfExpr {
        expr: var,
        ty: SdfExprType::Vec3,
    }
}

fn sdf_to_float(expr: SdfExpr) -> String {
    match expr.ty {
        SdfExprType::Float => expr.expr,
        SdfExprType::Vec3 => {
            format!("dot({}, vec3f(0.333333, 0.333333, 0.333333))", expr.expr)
        }
    }
}

fn sdf_to_vec3(expr: SdfExpr) -> String {
    match expr.ty {
        SdfExprType::Float => format!("vec3f({})", expr.expr),
        SdfExprType::Vec3 => expr.expr,
    }
}

fn sdf_result_type(a: SdfExprType, b: SdfExprType) -> SdfExprType {
    if a == SdfExprType::Vec3 || b == SdfExprType::Vec3 {
        SdfExprType::Vec3
    } else {
        SdfExprType::Float
    }
}

fn sdf_coerce(expr: &SdfExpr, ty: SdfExprType) -> String {
    match ty {
        SdfExprType::Float => match expr.ty {
            SdfExprType::Float => expr.expr.clone(),
            SdfExprType::Vec3 => {
                format!("dot({}, vec3f(0.333333, 0.333333, 0.333333))", expr.expr)
            }
        },
        SdfExprType::Vec3 => match expr.ty {
            SdfExprType::Float => format!("vec3f({})", expr.expr),
            SdfExprType::Vec3 => expr.expr.clone(),
        },
    }
}

/// PBR functions adapted for SDF renderer context (uses LIGHT_DIR/LIGHT_COLOR constants)
const PBR_SDF_FUNCTIONS: &str = r#"
fn D_GGX_sdf(NoH: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = (NoH * NoH) * (a2 - 1.0) + 1.0;
    return a2 / (3.14159265 * d * d + 0.00001);
}

fn F_Schlick_sdf(cosTheta: f32, F0: vec3f) -> vec3f {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

fn G_Smith_sdf(NoV: f32, NoL: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    let g1 = NoV / (NoV * (1.0 - k) + k);
    let g2 = NoL / (NoL * (1.0 - k) + k);
    return g1 * g2;
}

fn pbr_shade_sdf(
    base_color: vec3f, metallic: f32, roughness: f32,
    N: vec3f, V: vec3f, ao: f32, emission: vec3f
) -> vec3f {
    let L = LIGHT_DIR;
    let H = normalize(V + L);
    let NoL = max(dot(N, L), 0.0);
    let NoV = max(dot(N, V), 0.001);
    let NoH = max(dot(N, H), 0.0);
    let HoV = max(dot(H, V), 0.0);

    let F0 = mix(vec3f(0.04), base_color, metallic);
    let D = D_GGX_sdf(NoH, max(roughness, 0.04));
    let G = G_Smith_sdf(NoV, NoL, max(roughness, 0.04));
    let F = F_Schlick_sdf(HoV, F0);

    let spec = (D * G * F) / (4.0 * NoV * NoL + 0.0001);
    let kD = (vec3f(1.0) - F) * (1.0 - metallic);
    let diffuse = kD * base_color / 3.14159265;

    let direct = (diffuse + spec) * LIGHT_COLOR * NoL;
    let ambient = vec3f(AMBIENT) * base_color * ao;

    return ambient + direct + emission;
}

fn sdf_env_color(dir: vec3f) -> vec3f {
    let up = clamp(dir.y * 0.5 + 0.5, 0.0, 1.0);
    let base_low = clamp(BG_COLOR * vec3f(1.6, 1.8, 2.0) + vec3f(0.12, 0.16, 0.22), vec3f(0.0), vec3f(1.0));
    let base_high = clamp(BG_COLOR * vec3f(2.0, 2.2, 2.4) + vec3f(0.18, 0.22, 0.28), vec3f(0.0), vec3f(1.0));
    let base = mix(base_low, base_high, smoothstep(0.0, 1.0, up));
    let key_dir = normalize(LIGHT_DIR + vec3f(0.0, 0.22, 0.0));
    let fill_dir = normalize(vec3f(-LIGHT_DIR.x * 0.65, 0.38, -LIGHT_DIR.z * 0.65));
    let key = exp((dot(normalize(dir), key_dir) - 1.0) * 44.0) * 0.06;
    let fill = exp((dot(normalize(dir), fill_dir) - 1.0) * 28.0) * 0.02;
    let rim = exp(-pow(abs(dir.y) * 2.8, 2.0)) * 0.002;
    return clamp(base + LIGHT_COLOR * key + vec3f(1.0) * fill + vec3f(rim), vec3f(0.0), vec3f(1.0));
}

fn sdf_floor_color(uv: vec2f) -> vec3f {
    let radial = clamp(length(uv) * 0.072, 0.0, 1.0);
    let depth = smoothstep(-2.6, 6.8, uv.y);
    let center = clamp(BG_COLOR * vec3f(0.56, 0.70, 0.84) + vec3f(0.010, 0.018, 0.028), vec3f(0.0), vec3f(1.0));
    let edge = clamp(BG_COLOR * vec3f(0.34, 0.48, 0.64) + vec3f(0.002, 0.004, 0.008), vec3f(0.0), vec3f(1.0));
    let base = mix(center, edge, clamp(depth * 0.68 + radial * 0.46, 0.0, 1.0));
    let sheen = vec3f(exp(-dot(uv, uv) * 0.012) * 0.0008);
    return clamp(base + sheen, vec3f(0.0), vec3f(1.0));
}

fn sdf_scene_reflection_color(
    p: vec3f,
    rd: vec3f,
    N: vec3f,
    roughness: f32,
    transmission: f32
) -> vec3f {
    let gloss = clamp(1.0 - roughness * 0.75, 0.0, 1.0);
    let refl_dir = normalize(mix(reflect(rd, N), N, roughness * roughness * 0.1));
    let origin = p + N * mix(0.018, 0.04, transmission) + refl_dir * 0.01;
    let hit_t = ray_march_from(origin, refl_dir, 0.0, MAX_DIST);
    if hit_t > 0.0 {
        let hit_color = shade_probe(origin, refl_dir, hit_t);
        return mix(sdf_env_color(refl_dir), hit_color, gloss);
    }
    return sdf_env_color(refl_dir);
}

fn sdf_refract_thickness(p: vec3f, rd: vec3f, N: vec3f, ior: f32) -> f32 {
    let refr = refract(rd, N, 1.0 / ior);
    if length(refr) < 0.001 { return 0.0; }
    var pos = p + refr * 0.012;
    var dist = 0.0;
    for (var i = 0u; i < 40u; i++) {
        let d = sdf_scene(pos);
        if d > 0.0015 { break; }
        let step_len = clamp(abs(d) * 0.85 + 0.006, 0.006, 0.05);
        pos += refr * step_len;
        dist += step_len;
    }
    return dist;
}

fn sdf_transmission_light(
    p: vec3f,
    rd: vec3f,
    N: vec3f,
    ior: f32,
    roughness: f32,
    transmission: f32,
    thickness: f32,
    base_tint: vec3f
) -> vec3f {
    let L = normalize(LIGHT_DIR);
    let light_in = -L;
    let V = -rd;
    let side_bias = pow(1.0 - abs(N.y), 0.82);
    let thickness_factor = smoothstep(0.05, 0.42, thickness);
    let entry_n = sdf_refract_micro_normal(p, -N, thickness * 0.7, transmission);
    var light_refr = refract(light_in, -entry_n, 1.0 / ior);
    if length(light_refr) < 0.001 {
        light_refr = light_in;
    }
    let light_refr_n = normalize(light_refr);
    let through_view = pow(max(dot(light_refr_n, V), 0.0), mix(12.0, 5.0, roughness));
    let backlight = pow(max(dot(N, light_in), 0.0), 1.15);
    let through_light = pow(max(dot(V, light_in), 0.0), 1.35);
    let lateral_light = normalize(vec3f(light_refr.x, 0.0, light_refr.z) + vec3f(0.0001, 0.0, 0.0));
    let lateral_view = normalize(vec3f(V.x, 0.0, V.z) + vec3f(0.0001, 0.0, 0.0));
    let side_through = pow(max(dot(lateral_light, lateral_view), 0.0), mix(2.6, 1.5, roughness));
    let side_wrap = pow(max(dot(normalize(light_refr_n - N * dot(light_refr_n, N)), normalize(V - N * dot(V, N))), 0.0), mix(3.4, 1.9, roughness));
    let lateral_backlight = pow(1.0 - abs(dot(N, light_in)), 0.95);
    let forward_strength = transmission
        * thickness_factor
        * mix(0.14, 0.34, side_bias)
        * mix(0.58, 0.90, backlight);
    let lateral_strength = transmission
        * thickness_factor
        * mix(0.35, 1.0, side_bias)
        * mix(0.22, 0.72, lateral_backlight)
        * smoothstep(0.06, 0.30, thickness);
    let lateral_glow = transmission
        * thickness_factor
        * side_bias
        * lateral_backlight
        * 0.22;
    let tint = mix(vec3f(1.0), base_tint, 0.06);
    let forward_term = forward_strength * mix(through_light, through_view, 0.78);
    let lateral_term = lateral_strength * mix(side_through, side_wrap, 0.45);
    return LIGHT_COLOR * tint * (forward_term + lateral_term + lateral_glow);
}

fn sdf_refract_micro_normal(
    p: vec3f,
    N: vec3f,
    thickness: f32,
    transmission: f32
) -> vec3f {
    let side_bias = pow(1.0 - abs(N.y), 0.72);
    let q = p * mix(1.05, 2.05, transmission);
    let broad_q = p * mix(0.34, 0.88, transmission);
    let nx = shade_fbm_noise(q + vec3f(13.1, 1.7, 0.4)) * 2.0 - 1.0;
    let ny = shade_fbm_noise(q + vec3f(0.7, 11.3, 2.2)) * 2.0 - 1.0;
    let nz = shade_fbm_noise(q + vec3f(3.4, 0.9, 17.2)) * 2.0 - 1.0;
    let bx = shade_fbm_noise(broad_q + vec3f(2.3, 7.1, 1.9)) * 2.0 - 1.0;
    let bz = shade_fbm_noise(broad_q + vec3f(8.4, 0.6, 5.7)) * 2.0 - 1.0;
    var tangent = cross(vec3f(0.0, 1.0, 0.0), N);
    if length(tangent) < 0.001 {
        tangent = cross(vec3f(1.0, 0.0, 0.0), N);
    }
    tangent = normalize(tangent);
    let bitangent = normalize(cross(N, tangent));
    let thickness_factor = smoothstep(0.05, 0.34, thickness);
    let strength = clamp(
        thickness * mix(0.10, 0.015, transmission) + thickness_factor * side_bias * mix(0.015, 0.008, transmission),
        0.0,
        0.05
    );
    let warp = tangent * mix(nx, bx, 0.88) + bitangent * mix(nz, bz, 0.88) + N * ny * 0.02;
    return normalize(N + warp * strength);
}

fn sdf_refract_sample_env(sample_origin: vec3f, sample_dir: vec3f, fallback_pos: vec3f, travel_dist: f32) -> vec3f {
    var bg = sdf_env_color(sample_dir);
    if sample_dir.y < 0.0 {
        bg = sdf_floor_color(fallback_pos.xz + sample_dir.xz * (0.65 + travel_dist * 0.45));
    }
    return bg;
}

fn sdf_refract_sample_scene(sample_origin: vec3f, sample_dir: vec3f, fallback_pos: vec3f, travel_dist: f32) -> vec3f {
    var bg = sdf_env_color(sample_dir);
    let hit_t = ray_march_from(sample_origin + sample_dir * 0.025, sample_dir, 0.0, MAX_DIST);
    if hit_t > 0.0 {
        bg = shade_probe(sample_origin, sample_dir, hit_t);
    } else if sample_dir.y < 0.0 {
        bg = sdf_floor_color(fallback_pos.xz + sample_dir.xz * (0.65 + travel_dist * 0.45));
    }
    return bg;
}

fn sdf_refract_channel_env(
    p: vec3f,
    rd: vec3f,
    N: vec3f,
    ior_ch: f32,
    absorption: f32,
    thickness: f32,
    transmission: f32
) -> vec3f {
    let side_bias = pow(1.0 - abs(N.y), 0.82);
    let entry_n = sdf_refract_micro_normal(p, N, thickness, transmission);
    let refr = refract(rd, entry_n, 1.0 / ior_ch);
    if length(refr) < 0.001 { return sdf_env_color(reflect(rd, N)); }
    var pos = p + refr * 0.02;
    var dist = 0.0;
    var ray_dir = refr;
    for (var i = 0u; i < 30u; i++) {
        let d = sdf_scene(pos);
        if d > 0.01 { break; }
        let step_len = max(abs(d), 0.008);
        // Detect proximity to internal edges via SDF gradient
        let local_grad = abs(calc_normal(pos));
        let lg_max = max(max(local_grad.x, local_grad.y), local_grad.z);
        let lg_mid = local_grad.x + local_grad.y + local_grad.z - lg_max - min(min(local_grad.x, local_grad.y), local_grad.z);
        let near_edge_i = smoothstep(0.10, 0.50, lg_mid / max(lg_max, 0.001));
        let edge_mult = 1.0 + near_edge_i * 4.0;
        let n_bend = shade_fbm_noise(pos * 8.0 + vec3f(7.3, 2.1, 4.8));
        let bend = (n_bend - 0.5) * 0.08 * edge_mult;
        let n2 = shade_fbm_noise(pos * 4.0 + vec3f(1.4, 8.7, 3.2));
        let bend2 = (n2 - 0.5) * 0.05 * edge_mult;
        ray_dir = normalize(ray_dir + vec3f(bend, bend2, bend * 0.7 + bend2 * 0.3));
        pos += ray_dir * step_len;
        dist += step_len;
    }
    let exit_n = sdf_refract_micro_normal(pos, calc_normal(pos), thickness + dist * 0.25, transmission);
    let exit_rd = refract(ray_dir, -exit_n, ior_ch);
    var sample_dir = exit_rd;
    if length(sample_dir) < 0.001 {
        sample_dir = ray_dir;
    }
    let primary = sdf_refract_sample_env(pos, sample_dir, pos, dist);
    var col = primary * exp(-dist * absorption);

    let exit_abs = abs(calc_normal(pos));
    let exit_max_e = max(max(exit_abs.x, exit_abs.y), exit_abs.z);
    let exit_mid_e = exit_abs.x + exit_abs.y + exit_abs.z - exit_max_e - min(min(exit_abs.x, exit_abs.y), exit_abs.z);
    let exit_ratio_e = exit_mid_e / max(exit_max_e, 0.001);
    let edge_line_e = exp(-pow((exit_ratio_e - 0.45) / 0.10, 2.0)) * 0.50;
    col *= 1.0 - edge_line_e;

    let exit_fresnel = pow(1.0 - max(dot(-ray_dir, exit_n), 0.0), 2.4);
    let bounce_weight = clamp(exit_fresnel * transmission * (0.004 + side_bias * 0.025) * smoothstep(0.10, 0.45, dist), 0.0, 0.035);
    if bounce_weight > 0.001 {
        let bounce = reflect(refr, -exit_n);
        var bounce_pos = pos + bounce * 0.02;
        var bounce_dist = 0.0;
        for (var i = 0u; i < 24u; i++) {
            let d = sdf_scene(bounce_pos);
            if d > 0.01 { break; }
            bounce_pos += bounce * max(abs(d), 0.008);
            bounce_dist += max(abs(d), 0.008);
        }
        let bounce_exit_n = sdf_refract_micro_normal(
            bounce_pos,
            calc_normal(bounce_pos),
            thickness + (dist + bounce_dist) * 0.35,
            transmission
        );
        var bounce_dir = refract(bounce, -bounce_exit_n, ior_ch);
        if length(bounce_dir) < 0.001 {
            bounce_dir = reflect(bounce, -bounce_exit_n);
        }
        let bounced = sdf_refract_sample_env(bounce_pos, bounce_dir, bounce_pos, dist + bounce_dist);
        let bounced_col = bounced * exp(-(dist + bounce_dist) * absorption * 1.08);
        col = mix(col, bounced_col, bounce_weight);
    }
    return col;
}

fn sdf_refract_channel_scene(
    p: vec3f,
    rd: vec3f,
    N: vec3f,
    ior_ch: f32,
    absorption: f32,
    thickness: f32,
    transmission: f32
) -> vec3f {
    let side_bias = pow(1.0 - abs(N.y), 0.82);
    let entry_n = sdf_refract_micro_normal(p, N, thickness, transmission);
    let refr = refract(rd, entry_n, 1.0 / ior_ch);
    if length(refr) < 0.001 { return sdf_env_color(reflect(rd, N)); }
    var pos = p + refr * 0.02;
    var dist = 0.0;
    var ray_dir = refr;
    for (var i = 0u; i < 30u; i++) {
        let d = sdf_scene(pos);
        if d > 0.01 { break; }
        let step_len = max(abs(d), 0.008);
        // Internal ice structure: noise-based concavities bend light
        // Detect proximity to internal edges via SDF gradient
        let local_grad = abs(calc_normal(pos));
        let lg_max = max(max(local_grad.x, local_grad.y), local_grad.z);
        let lg_mid = local_grad.x + local_grad.y + local_grad.z - lg_max - min(min(local_grad.x, local_grad.y), local_grad.z);
        let near_edge_i = smoothstep(0.10, 0.50, lg_mid / max(lg_max, 0.001));
        let edge_mult = 1.0 + near_edge_i * 4.0;
        let n_bend = shade_fbm_noise(pos * 8.0 + vec3f(7.3, 2.1, 4.8));
        let bend = (n_bend - 0.5) * 0.08 * edge_mult;
        let n2 = shade_fbm_noise(pos * 4.0 + vec3f(1.4, 8.7, 3.2));
        let bend2 = (n2 - 0.5) * 0.05 * edge_mult;
        ray_dir = normalize(ray_dir + vec3f(bend, bend2, bend * 0.7 + bend2 * 0.3));
        pos += ray_dir * step_len;
        dist += step_len;
    }
    let exit_n = sdf_refract_micro_normal(pos, calc_normal(pos), thickness + dist * 0.25, transmission);
    let exit_rd = refract(ray_dir, -exit_n, ior_ch);
    var sample_dir = exit_rd;
    if length(sample_dir) < 0.001 {
        sample_dir = ray_dir;
    }
    let primary = sdf_refract_sample_scene(pos, sample_dir, pos, dist);
    var col = primary * exp(-dist * absorption);

    // Internal edge detection: where exit normal is between faces, darken
    let exit_abs = abs(calc_normal(pos));
    let exit_max = max(max(exit_abs.x, exit_abs.y), exit_abs.z);
    let exit_mid = exit_abs.x + exit_abs.y + exit_abs.z - exit_max - min(min(exit_abs.x, exit_abs.y), exit_abs.z);
    let exit_ratio = exit_mid / max(exit_max, 0.001);
    let edge_line = exp(-pow((exit_ratio - 0.45) / 0.10, 2.0)) * 0.50;
    col *= 1.0 - edge_line;

    let face_edge = pow(1.0 - max(dot(-rd, N), 0.0), 1.6);
    let interior_lobe_weight = clamp(
        transmission * (0.02 + face_edge * 0.06 + side_bias * 0.04) * smoothstep(0.08, 0.30, dist),
        0.0,
        0.10
    );
    if interior_lobe_weight > 0.001 {
        let lobe_origin = p + refr * clamp(dist * 0.45, 0.035, 0.34);
        let lobe_n = sdf_refract_micro_normal(
            lobe_origin,
            normalize(mix(entry_n, exit_n, 0.35)),
            thickness + dist * 0.45,
            transmission
        );
        let lobe_dir = normalize(mix(sample_dir, reflect(refr, lobe_n), 0.24 + side_bias * 0.18));
        let lobe = sdf_refract_sample_scene(lobe_origin, lobe_dir, lobe_origin, dist * 0.75);
        let lobe_col = lobe * exp(-dist * absorption * 0.92);
        col = mix(col, lobe_col, interior_lobe_weight);
    }

    let wall_face_weight = clamp(
        transmission * (0.02 + face_edge * 0.05 + side_bias * 0.03) * smoothstep(0.08, 0.35, dist),
        0.0,
        0.08
    );
    if wall_face_weight > 0.001 {
        let wall_origin = p + refr * clamp(dist * 0.18, 0.028, 0.14);
        let wall_n = normalize(mix(entry_n, -exit_n, 0.46));
        let wall_dir = normalize(reflect(refr, wall_n));
        let wall_t = ray_march_from(
            wall_origin + wall_dir * 0.02,
            wall_dir,
            0.0,
            clamp(dist * 1.55 + thickness * 1.10, 0.28, 1.0)
        );
        if wall_t > 0.0 {
            let wall_face = shade_probe(wall_origin, wall_dir, wall_t);
            col = mix(col, wall_face, wall_face_weight);
        }
    }

    let exit_fresnel = pow(1.0 - max(dot(-refr, exit_n), 0.0), 2.4);
    let bounce_weight = clamp(
        transmission * (0.01 + exit_fresnel * 0.08 + side_bias * 0.04) * smoothstep(0.08, 0.35, dist),
        0.0,
        0.10
    );
    if bounce_weight > 0.001 {
        let bounce = reflect(refr, -exit_n);
        var bounce_pos = pos + bounce * 0.02;
        var bounce_dist = 0.0;
        for (var i = 0u; i < 24u; i++) {
            let d = sdf_scene(bounce_pos);
            if d > 0.01 { break; }
            bounce_pos += bounce * max(abs(d), 0.008);
            bounce_dist += max(abs(d), 0.008);
        }
        let bounce_exit_n = sdf_refract_micro_normal(
            bounce_pos,
            calc_normal(bounce_pos),
            thickness + (dist + bounce_dist) * 0.35,
            transmission
        );
        var bounce_dir = refract(bounce, -bounce_exit_n, ior_ch);
        if length(bounce_dir) < 0.001 {
            bounce_dir = reflect(bounce, -bounce_exit_n);
        }
        let bounced = sdf_refract_sample_scene(bounce_pos, bounce_dir, bounce_pos, dist + bounce_dist);
        let bounced_col = bounced * exp(-(dist + bounce_dist) * absorption * 1.08);
        col = mix(col, bounced_col, bounce_weight);
    }
    return col;
}

fn sdf_refract_color_env(
    p: vec3f,
    rd: vec3f,
    N: vec3f,
    ior: f32,
    base_tint: vec3f,
    transmission: f32,
    thickness: f32
) -> vec3f {
    let side_bias = pow(1.0 - abs(N.y), 0.82);
    let thickness_factor = smoothstep(0.05, 0.42, thickness);
    let prism_factor = clamp(thickness_factor * mix(0.72, 1.0, side_bias), 0.0, 1.0);
    let dispersion = mix(0.0008, 0.010, prism_factor) * mix(0.9, 1.16, transmission);
    let absorption = -log(max(base_tint, vec3f(0.01))) * mix(0.5, 1.4, transmission);
    let col_r = sdf_refract_channel_env(p, rd, N, max(1.01, ior - dispersion), absorption.x, thickness, transmission);
    let col_g = sdf_refract_channel_env(p, rd, N, ior, absorption.y, thickness, transmission);
    let col_b = sdf_refract_channel_env(p, rd, N, ior + dispersion, absorption.z, thickness, transmission);
    let col = vec3f(col_r.x, col_g.y, col_b.z);
    let tint_strength = clamp(thickness * mix(0.05, 0.15, transmission), 0.0, 0.25);
    return col * mix(vec3f(1.0), base_tint, tint_strength);
}

fn sdf_refract_color_scene(
    p: vec3f,
    rd: vec3f,
    N: vec3f,
    ior: f32,
    base_tint: vec3f,
    transmission: f32,
    thickness: f32
) -> vec3f {
    let side_bias = pow(1.0 - abs(N.y), 0.82);
    let thickness_factor = smoothstep(0.05, 0.42, thickness);
    let prism_factor = clamp(thickness_factor * mix(0.72, 1.0, side_bias), 0.0, 1.0);
    let dispersion = mix(0.0008, 0.010, prism_factor) * mix(0.9, 1.16, transmission);
    let absorption = -log(max(base_tint, vec3f(0.01))) * mix(0.5, 1.4, transmission);
    let col_r = sdf_refract_channel_scene(p, rd, N, max(1.01, ior - dispersion), absorption.x, thickness, transmission);
    let col_g = sdf_refract_channel_scene(p, rd, N, ior, absorption.y, thickness, transmission);
    let col_b = sdf_refract_channel_scene(p, rd, N, ior + dispersion, absorption.z, thickness, transmission);
    let col = vec3f(col_r.x, col_g.y, col_b.z);
    let tint_strength = clamp(thickness * mix(0.05, 0.15, transmission), 0.0, 0.25);
    return col * mix(vec3f(1.0), base_tint, tint_strength);
}
"#;

const SDF_SHADE_NOISE_FUNCTIONS: &str = r#"
fn shade_hash3(p: vec3f) -> f32 {
    var q = fract(p * 0.1031);
    q = q + dot(q, q.yzx + 19.19);
    return fract((q.x + q.y) * q.z);
}

fn shade_noise3d(p: vec3f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(mix(shade_hash3(i), shade_hash3(i + vec3f(1.0, 0.0, 0.0)), u.x),
            mix(shade_hash3(i + vec3f(0.0, 1.0, 0.0)), shade_hash3(i + vec3f(1.0, 1.0, 0.0)), u.x), u.y),
        mix(mix(shade_hash3(i + vec3f(0.0, 0.0, 1.0)), shade_hash3(i + vec3f(1.0, 0.0, 1.0)), u.x),
            mix(shade_hash3(i + vec3f(0.0, 1.0, 1.0)), shade_hash3(i + vec3f(1.0, 1.0, 1.0)), u.x), u.y), u.z);
}

fn shade_fbm_noise(p: vec3f) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var q = p;
    for (var i = 0u; i < 4u; i = i + 1u) {
        value = value + amplitude * shade_noise3d(q);
        q = q * 2.0;
        amplitude = amplitude * 0.5;
    }
    return value;
}

fn shade_voronoi_noise(p: vec3f) -> f32 {
    let n = floor(p);
    let f = fract(p);
    var md = 8.0;
    for (var k = -1; k <= 1; k++) {
        for (var j = -1; j <= 1; j++) {
            for (var i = -1; i <= 1; i++) {
                let g = vec3f(f32(i), f32(j), f32(k));
                let o = vec3f(
                    shade_hash3(n + g),
                    shade_hash3(n + g + 17.0),
                    shade_hash3(n + g + 31.0)
                );
                let r = g + o - f;
                md = min(md, dot(r, r));
            }
        }
    }
    return sqrt(md);
}
"#;

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

// Shadow map uniforms (binding group 1, optional)
const SHADOW_UNIFORMS: &str = r#"
struct ShadowUniforms {
    light_vp: mat4x4f,
    shadow_bias: f32,
    shadow_enabled: f32,
    _pad: vec2f,
};

@group(1) @binding(0) var<uniform> u_shadow: ShadowUniforms;
@group(1) @binding(1) var shadow_map: texture_2d<f32>;
@group(1) @binding(2) var shadow_sampler: sampler;

fn sample_shadow(world_pos: vec3f) -> f32 {
    if u_shadow.shadow_enabled < 0.5 { return 1.0; }
    let light_space = u_shadow.light_vp * vec4f(world_pos, 1.0);
    let proj = light_space.xyz / light_space.w;
    let uv = proj.xy * 0.5 + 0.5;
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 { return 1.0; }
    let shadow_depth = textureSample(shadow_map, shadow_sampler, uv).r;
    let current_depth = proj.z;
    // PCF 3x3 soft shadow
    let texel_size = 1.0 / 1024.0;
    var shadow = 0.0;
    for (var x = -1; x <= 1; x++) {
        for (var y = -1; y <= 1; y++) {
            let offset = vec2f(f32(x), f32(y)) * texel_size;
            let pcf_depth = textureSample(shadow_map, shadow_sampler, uv + offset).r;
            shadow += select(0.0, 1.0, current_depth - u_shadow.shadow_bias <= pcf_depth);
        }
    }
    return shadow / 9.0;
}
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
            // Directional light (with shadow)
            let L = normalize(-light.direction);
            let shadow = select(1.0, sample_shadow(world_pos), light.cast_shadow > 0.5);
            result += pbr_direct(base_color, metallic, roughness, N, V, L, light_color * shadow);
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
                base_color: Box::new(ConstVec3 { c: [0.8, 0.2, 0.1] }),
                metallic: Box::new(ConstFloat { c: 0.0 }),
                roughness: Box::new(ConstFloat { c: 0.5 }),
                normal: None,
                emission: Box::new(ConstVec3 { c: [0.0, 0.0, 0.0] }),
                emission_strength: Box::new(ConstFloat { c: 0.0 }),
                ao: None,
                alpha: Box::new(ConstFloat { c: 1.0 }),
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
        println!(
            "Fragment WGSL ({} bytes):\n{}",
            mat.fragment_wgsl.len(),
            &mat.fragment_wgsl[..500.min(mat.fragment_wgsl.len())]
        );

        // Also test SDF shade compilation
        let shade = compile_sdf_shade(&tree);
        println!(
            "SDF shade WGSL ({} bytes):\n{}",
            shade.len(),
            &shade[..200.min(shade.len())]
        );
        assert!(shade.contains("fn shade("));
        assert!(shade.contains("fn shade_probe("));
        assert!(shade.contains("sdf_refract_color_env("));
        assert!(shade.contains("sdf_refract_color_scene("));
        assert!(shade.contains("pbr_shade_sdf"));
    }

    #[test]
    fn test_sdf_shade_from_actor_json() {
        let json_str = r#"{
            "type": "materialOutput",
            "surface": {
                "type": "principledBsdf",
                "base_color": {
                    "type": "colorMix",
                    "mode": "mix",
                    "fac": {"type": "noiseTexture", "scale": {"type": "constFloat", "c": 5.0}, "detail": {"type": "constFloat", "c": 2.0}, "roughness": {"type": "constFloat", "c": 0.5}},
                    "a": {"type": "constVec3", "c": [0.75, 0.88, 0.95]},
                    "b": {"type": "constVec3", "c": [0.4, 0.6, 0.8]}
                },
                "metallic": {"type": "constFloat", "c": 0.0},
                "roughness": {"type": "constFloat", "c": 0.08},
                "emission": {"type": "constVec3", "c": [0.1, 0.15, 0.2]},
                "emission_strength": {"type": "constFloat", "c": 0.2},
                "alpha": {"type": "constFloat", "c": 0.9},
                "ior": {"type": "constFloat", "c": 1.31}
            }
        }"#;
        let node: crate::ir::ShaderNode = serde_json::from_str(json_str).expect("deser failed");
        let shade = compile_sdf_shade(&node);
        println!(
            "SDF shade from actor JSON ({} bytes):\n{}",
            shade.len(),
            &shade[..300.min(shade.len())]
        );
        assert!(shade.contains("fn shade("));
        assert!(shade.contains("fn shade_probe("));
        assert!(shade.contains("sdf_refract_color_env("));
    }

    #[test]
    fn test_compile_noise_material() {
        let tree = MaterialOutput {
            surface: Box::new(PrincipledBsdf {
                base_color: Box::new(NoiseTexture {
                    scale: Box::new(ConstFloat { c: 5.0 }),
                    detail: Box::new(ConstFloat { c: 2.0 }),
                    roughness: Box::new(ConstFloat { c: 0.5 }),
                }),
                metallic: Box::new(ConstFloat { c: 0.8 }),
                roughness: Box::new(ConstFloat { c: 0.2 }),
                normal: None,
                emission: Box::new(ConstVec3 { c: [0.0, 0.0, 0.0] }),
                emission_strength: Box::new(ConstFloat { c: 0.0 }),
                ao: None,
                alpha: Box::new(ConstFloat { c: 1.0 }),
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

    #[test]
    fn test_compile_sdf_bump_normal_material() {
        let tree = MaterialOutput {
            surface: Box::new(PrincipledBsdf {
                base_color: Box::new(ConstVec3 {
                    c: [0.9, 0.96, 1.0],
                }),
                metallic: Box::new(ConstFloat { c: 0.0 }),
                roughness: Box::new(ConstFloat { c: 0.03 }),
                normal: Some(Box::new(BumpMap {
                    strength: Box::new(ConstFloat { c: 0.15 }),
                    height: Box::new(SeparateXYZ {
                        input: Box::new(NoiseTexture {
                            scale: Box::new(ConstFloat { c: 6.0 }),
                            detail: Box::new(ConstFloat { c: 2.0 }),
                            roughness: Box::new(ConstFloat { c: 0.5 }),
                        }),
                        component: "x".to_string(),
                    }),
                })),
                emission: Box::new(ConstVec3 { c: [0.0, 0.0, 0.0] }),
                emission_strength: Box::new(ConstFloat { c: 0.0 }),
                ao: None,
                alpha: Box::new(ConstFloat { c: 0.02 }),
                subsurface: None,
                subsurface_color: None,
                clearcoat: None,
                clearcoat_roughness: None,
                anisotropic: None,
                anisotropic_rotation: None,
                sheen: None,
                sheen_tint: None,
                transmission: Some(Box::new(ConstFloat { c: 0.99 })),
                ior: Some(Box::new(ConstFloat { c: 1.31 })),
            }),
        };

        let shade = compile_sdf_shade(&tree);
        assert!(shade.contains("_dhdx"));
        assert!(shade.contains("shade_fbm_noise"));
        assert!(shade.contains("cross("));
        assert!(shade.contains("shade_probe"));
        assert!(shade.contains("sdf_refract_color_scene("));
    }
}
