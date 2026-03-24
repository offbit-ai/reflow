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

        // ═══ Principled BSDF — multi-light PBR shading with advanced lobes ═══
        ShaderNode::PrincipledBsdf {
            base_color, metallic, roughness, normal,
            emission, emission_strength, ao, alpha,
            clearcoat, clearcoat_roughness,
            subsurface, subsurface_color,
            sheen, sheen_tint,
            anisotropic, anisotropic_rotation,
            transmission, ior,
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

            // Advanced lobes
            let vcc = clearcoat.as_ref().map(|n| emit_node(ctx, n)).unwrap_or_else(|| "0.0".to_string());
            let vccr = clearcoat_roughness.as_ref().map(|n| emit_node(ctx, n)).unwrap_or_else(|| "0.03".to_string());
            let vsss = subsurface.as_ref().map(|n| emit_node(ctx, n)).unwrap_or_else(|| "0.0".to_string());
            let vsss_c = subsurface_color.as_ref().map(|n| emit_node(ctx, n)).unwrap_or_else(|| vbc.clone());
            let vsh = sheen.as_ref().map(|n| emit_node(ctx, n)).unwrap_or_else(|| "0.0".to_string());
            let vsh_t = sheen_tint.as_ref().map(|n| emit_node(ctx, n)).unwrap_or_else(|| "0.5".to_string());

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
/// Compile a ShaderNode tree into a `fn shade(ro, rd, t) -> vec3f` WGSL string
/// for the SDF ray march renderer. Extracts material parameters from the IR
/// and generates a shade function using SDF-context variables.
pub fn compile_sdf_shade(root: &ShaderNode) -> String {
    // Extract PBR parameters from the IR tree
    let (base_color, metallic, roughness, emission, emission_str, ior) = extract_pbr_params(root);

    let mut shade = String::new();
    shade.push_str(PBR_SDF_FUNCTIONS);

    shade.push_str("fn shade(ro: vec3f, rd: vec3f, t: f32) -> vec3f {\n");
    shade.push_str("    let p = ro + rd * t;\n");
    shade.push_str("    let N = calc_normal(p);\n");
    shade.push_str("    let V = -rd;\n");
    shade.push_str("    let ao = calc_ao(p, N);\n");
    shade.push_str("\n");

    shade.push_str(&format!("    let base_color = {};\n", base_color));
    shade.push_str(&format!("    let roughness = {:.4};\n", roughness));
    shade.push_str(&format!("    let ior = {:.4};\n", ior));

    // Detect puddle vs ice
    shade.push_str("    let is_puddle = N.y > 0.85 && p.y < -0.4;\n");
    shade.push_str("    var col: vec3f;\n");
    shade.push_str("\n");

    shade.push_str("    if is_puddle {\n");
    // ── PUDDLE: wet reflective surface ──
    shade.push_str("        let refl_rd = reflect(rd, N);\n");
    shade.push_str("        let refl_t = ray_march(p + N * 0.02, refl_rd);\n");
    shade.push_str("        var refl_col: vec3f;\n");
    shade.push_str("        if refl_t > 0.0 {\n");
    shade.push_str("            let rp = p + N * 0.02 + refl_rd * refl_t;\n");
    shade.push_str("            let rn = calc_normal(rp);\n");
    shade.push_str("            refl_col = vec3f(0.8, 0.85, 0.9) * (0.3 + max(dot(rn, LIGHT_DIR), 0.0) * 0.6);\n");
    shade.push_str("        } else {\n");
    shade.push_str("            refl_col = mix(vec3f(0.7, 0.75, 0.82), vec3f(0.92, 0.94, 0.97), refl_rd.y * 0.5 + 0.5);\n");
    shade.push_str("        }\n");
    shade.push_str("        let wf = 0.15 + 0.85 * pow(1.0 - max(dot(N, -rd), 0.0), 3.0);\n");
    shade.push_str("        let water_spec = pow(max(dot(reflect(-LIGHT_DIR, N), -rd), 0.0), 128.0);\n");
    shade.push_str("        let water_base = vec3f(0.5, 0.72, 0.85) * (0.4 + max(dot(N, LIGHT_DIR), 0.0) * 0.5);\n");
    shade.push_str("        col = mix(water_base, refl_col, wf) + vec3f(water_spec * 0.8);\n");

    shade.push_str("    } else {\n");
    // ── ICE: mostly transparent with internal scattering + specular ──
    shade.push_str("        let refr = sdf_refract_color(p, rd, N, ior, base_color);\n");
    shade.push_str("        let NoV = max(dot(N, -rd), 0.0);\n");
    shade.push_str("        let fresnel = 0.04 + 0.96 * pow(1.0 - NoV, 5.0);\n");
    shade.push_str("        // Sharp specular\n");
    shade.push_str("        let H = normalize(LIGHT_DIR - rd);\n");
    shade.push_str("        let spec = pow(max(dot(N, H), 0.0), 128.0) * 1.5;\n");
    shade.push_str("        // Environment reflection at edges\n");
    shade.push_str("        let refl_rd = reflect(rd, N);\n");
    shade.push_str("        let env = mix(BG_COLOR * 1.2, vec3f(0.7, 0.8, 0.9), refl_rd.y * 0.5 + 0.5);\n");
    shade.push_str("        // Refraction is primary — fresnel blends reflection at edges\n");
    shade.push_str("        col = mix(refr, env, fresnel) * max(ao, 0.7) + vec3f(spec);\n");
    shade.push_str("    }\n");

    shade.push_str("    return col;\n");
    shade.push_str("}\n\n");
    shade
}

/// Extract PBR parameters from a ShaderNode IR tree as WGSL expressions
fn extract_pbr_params(root: &ShaderNode) -> (String, f32, f32, String, f32, f32) {
    match root {
        ShaderNode::MaterialOutput { surface } => extract_pbr_params(surface),
        ShaderNode::PrincipledBsdf {
            base_color, metallic, roughness,
            emission, emission_strength,
            ior, ..
        } => {
            let bc = extract_color_expr(base_color);
            let m = extract_float(metallic);
            let r = extract_float(roughness);
            let em = extract_color_expr(emission);
            let es = extract_float(emission_strength);
            let i = ior.as_ref().map(|n| extract_float(n)).unwrap_or(1.31);
            (bc, m, r, em, es, i)
        }
        _ => ("vec3f(0.8, 0.8, 0.8)".to_string(), 0.0, 0.5, "vec3f(0.0)".to_string(), 0.0, 1.31),
    }
}

fn extract_float(node: &ShaderNode) -> f32 {
    match node {
        ShaderNode::ConstFloat { c: v } => *v,
        _ => 0.5,
    }
}

fn extract_color_expr(node: &ShaderNode) -> String {
    match node {
        ShaderNode::ConstVec3 { c: v } => format!("vec3f({:.4}, {:.4}, {:.4})", v[0], v[1], v[2]),
        ShaderNode::ConstFloat { c: v } => format!("vec3f({:.4})", v),
        ShaderNode::NoiseTexture { .. } => {
            // Generate inline noise expression using world position
            "vec3f(noise3d(p * 5.0) * 0.3 + 0.7)".to_string()
        }
        ShaderNode::ColorMix { fac, a, b, .. } => {
            let fa = extract_color_expr(a);
            let fb = extract_color_expr(b);
            let ff = extract_color_expr(fac);
            format!("mix({fa}, {fb}, {ff})")
        }
        _ => "vec3f(0.8, 0.8, 0.8)".to_string(),
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

// Refraction: march through ice volume, exit out the back, sample background
// Chromatic dispersion applied at edges via IOR offset per channel
fn sdf_refract_channel(p: vec3f, rd: vec3f, N: vec3f, ior_ch: f32) -> vec3f {
    let refr = refract(rd, N, 1.0 / ior_ch);
    if length(refr) < 0.001 { return BG_COLOR; }
    // March through ice volume to exit point
    var pos = p + refr * 0.02;
    var dist = 0.0;
    for (var i = 0u; i < 30u; i++) {
        let d = sdf_scene(pos);
        if d > 0.01 { break; }
        pos += refr * max(abs(d), 0.008);
        dist += max(abs(d), 0.008);
    }
    let exit_n = calc_normal(pos);
    let exit_rd = refract(refr, -exit_n, ior_ch);
    // What you see through the ice — ground below, sky above
    var bg: vec3f;
    if exit_rd.y < 0.0 {
        // Looking down through ice → ground surface (light blue)
        bg = vec3f(0.5, 0.72, 0.86);
    } else {
        // Looking up/sideways → sky background
        bg = mix(BG_COLOR, vec3f(0.75, 0.88, 0.95), exit_rd.y);
    }
    let absorption = exp(-dist * 0.3);
    return bg * absorption;
}

fn sdf_refract_color(p: vec3f, rd: vec3f, N: vec3f, ior: f32, base_tint: vec3f) -> vec3f {
    // Chromatic dispersion: offset IOR per channel for edge rainbow
    let col_r = sdf_refract_channel(p, rd, N, ior - 0.03);
    let col_g = sdf_refract_channel(p, rd, N, ior);
    let col_b = sdf_refract_channel(p, rd, N, ior + 0.03);
    let col = vec3f(col_r.x, col_g.y, col_b.z);
    return col * mix(vec3f(1.0), base_tint, 0.12);
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
        println!("Fragment WGSL ({} bytes):\n{}", mat.fragment_wgsl.len(), &mat.fragment_wgsl[..500.min(mat.fragment_wgsl.len())]);

        // Also test SDF shade compilation
        let shade = compile_sdf_shade(&tree);
        println!("SDF shade WGSL ({} bytes):\n{}", shade.len(), &shade[..200.min(shade.len())]);
        assert!(shade.contains("fn shade("));
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
        println!("SDF shade from actor JSON ({} bytes):\n{}", shade.len(), &shade[..300.min(shade.len())]);
        assert!(shade.contains("fn shade("));
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
}
