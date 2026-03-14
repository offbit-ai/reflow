//! WGSL code generation from SDF IR trees.
//!
//! Walks the [`SdfNode`] tree and emits a complete WGSL compute shader
//! that ray marches the SDF scene. The output is a standalone shader
//! suitable for `wgpu` or WebGPU.

use crate::ir::*;

/// Compiled shader output.
pub struct CompiledShader {
    /// Complete WGSL source code.
    pub wgsl: String,
    /// Number of SDF function calls (for complexity estimation).
    pub node_count: u32,
    /// Whether the shader uses noise/displacement.
    pub uses_noise: bool,
    /// Whether the shader uses smooth operations.
    pub uses_smooth_ops: bool,
}

/// Compile an SdfNode tree into a complete WGSL ray marching shader.
pub fn compile(node: &SdfNode) -> CompiledShader {
    let mut ctx = CodegenContext::new();

    // Extract scene settings or use defaults
    let (root, settings) = match node {
        SdfNode::Scene { root, settings } => (root.as_ref(), settings.clone()),
        other => (other, SceneSettings::default()),
    };

    // Generate the SDF function body
    let sdf_expr = ctx.emit_node(root);

    // Build the complete shader
    let wgsl = build_shader(&ctx, &sdf_expr, &settings);

    CompiledShader {
        wgsl,
        node_count: ctx.counter,
        uses_noise: ctx.uses_noise,
        uses_smooth_ops: ctx.uses_smooth_ops,
    }
}

struct CodegenContext {
    /// Monotonic counter for unique variable names.
    counter: u32,
    /// Lines of code in the sdf_scene function body.
    lines: Vec<String>,
    /// Material assignments: var_name → material index.
    materials: Vec<SdfMaterial>,
    /// Whether noise functions are needed.
    uses_noise: bool,
    /// Whether smooth min/max are needed.
    uses_smooth_ops: bool,
}

impl CodegenContext {
    fn new() -> Self {
        Self {
            counter: 0,
            lines: Vec::new(),
            materials: Vec::new(),
            uses_noise: false,
            uses_smooth_ops: false,
        }
    }

    fn next_var(&mut self) -> String {
        let name = format!("d{}", self.counter);
        self.counter += 1;
        name
    }

    fn add_material(&mut self, mat: &SdfMaterial) -> usize {
        // Reuse existing if identical (by serialization)
        let json = serde_json::to_string(mat).unwrap_or_default();
        for (i, existing) in self.materials.iter().enumerate() {
            if serde_json::to_string(existing).unwrap_or_default() == json {
                return i;
            }
        }
        self.materials.push(mat.clone());
        self.materials.len() - 1
    }

    /// Emit code for a node, return the variable name holding the distance.
    fn emit_node(&mut self, node: &SdfNode) -> String {
        match node {
            SdfNode::Primitive { shape, .. } => self.emit_primitive(shape),
            SdfNode::Operation { op, left, right } => {
                let l = self.emit_node(left);
                let r = self.emit_node(right);
                self.emit_op(op, &l, &r)
            }
            SdfNode::Transform { transform, child } => self.emit_transform(transform, child),
            SdfNode::Material { child, .. } => {
                // Material doesn't change the distance — pass through
                self.emit_node(child)
            }
            SdfNode::Ref { name } => {
                // Named reference — assume the function exists
                let var = self.next_var();
                self.lines.push(format!("  let {} = sdf_{}(p);", var, name));
                var
            }
            SdfNode::Scene { root, .. } => self.emit_node(root),
        }
    }

    fn emit_primitive(&mut self, shape: &SdfPrimitive) -> String {
        let var = self.next_var();
        let expr = match shape {
            SdfPrimitive::Sphere { radius } => {
                format!("length(p) - {:.6}", radius)
            }
            SdfPrimitive::Box { size } => {
                format!("sdf_box(p, vec3f({:.6}, {:.6}, {:.6}))", size[0], size[1], size[2])
            }
            SdfPrimitive::RoundBox { size, radius } => {
                format!(
                    "sdf_box(p, vec3f({:.6}, {:.6}, {:.6})) - {:.6}",
                    size[0], size[1], size[2], radius
                )
            }
            SdfPrimitive::Cylinder { radius, height } => {
                format!("sdf_cylinder(p, {:.6}, {:.6})", *radius, *height)
            }
            SdfPrimitive::Capsule { radius, height } => {
                format!("sdf_capsule(p, {:.6}, {:.6})", *height, *radius)
            }
            SdfPrimitive::Torus { major_radius, minor_radius } => {
                format!("sdf_torus(p, {:.6}, {:.6})", *major_radius, *minor_radius)
            }
            SdfPrimitive::Cone { angle, height } => {
                format!("sdf_cone(p, {:.6}, {:.6})", *angle, *height)
            }
            SdfPrimitive::Plane { normal, offset } => {
                format!(
                    "dot(p, vec3f({:.6}, {:.6}, {:.6})) + {:.6}",
                    normal[0], normal[1], normal[2], offset
                )
            }
            SdfPrimitive::InfRepeat { spacing } => {
                format!(
                    "length(p - round(p / vec3f({:.6}, {:.6}, {:.6})) * vec3f({0:.6}, {1:.6}, {2:.6}))",
                    spacing[0], spacing[1], spacing[2]
                )
            }
        };
        self.lines.push(format!("  let {} = {};", var, expr));
        var
    }

    fn emit_op(&mut self, op: &SdfOp, left: &str, right: &str) -> String {
        let var = self.next_var();
        let expr = match op {
            SdfOp::Union => format!("min({}, {})", left, right),
            SdfOp::Intersection => format!("max({}, {})", left, right),
            SdfOp::Difference => format!("max({}, -{})", left, right),
            SdfOp::SmoothUnion { k } => {
                self.uses_smooth_ops = true;
                format!("smin({}, {}, {:.6})", left, right, k)
            }
            SdfOp::SmoothIntersection { k } => {
                self.uses_smooth_ops = true;
                format!("smax({}, {}, {:.6})", left, right, k)
            }
            SdfOp::SmoothDifference { k } => {
                self.uses_smooth_ops = true;
                format!("smax({}, -{}, {:.6})", left, right, k)
            }
        };
        self.lines.push(format!("  let {} = {};", var, expr));
        var
    }

    fn emit_transform(&mut self, transform: &SdfTransform, child: &SdfNode) -> String {
        match transform {
            SdfTransform::Translate { offset } => {
                let pvar = self.next_var();
                self.lines.push(format!(
                    "  let {pv} = p - vec3f({:.6}, {:.6}, {:.6});",
                    offset[0], offset[1], offset[2], pv = pvar
                ));
                self.lines.push("  { let p = ".to_string() + &pvar + ";");
                let result = self.emit_node(child);
                self.lines.push("  }".to_string());
                // The variable is still accessible after the block
                result
            }
            SdfTransform::Rotate { angles } => {
                let pvar = self.next_var();
                let [ax, ay, az] = *angles;
                self.lines.push(format!(
                    "  let {pv} = rot_xyz(p, vec3f({:.6}, {:.6}, {:.6}));",
                    ax.to_radians(), ay.to_radians(), az.to_radians(), pv = pvar
                ));
                self.lines.push(format!("  {{ let p = {};", pvar));
                let result = self.emit_node(child);
                self.lines.push("  }".to_string());
                result
            }
            SdfTransform::Scale { factor } => {
                // For uniform scale, we can divide p and multiply the result
                let s = factor[0]; // TODO: non-uniform scale needs more care
                let pvar = self.next_var();
                self.lines.push(format!("  let {} = p / {:.6};", pvar, s));
                self.lines.push(format!("  {{ let p = {};", pvar));
                let inner = self.emit_node(child);
                self.lines.push("  }".to_string());
                let var = self.next_var();
                self.lines.push(format!("  let {} = {} * {:.6};", var, inner, s));
                var
            }
            SdfTransform::Twist { strength } => {
                let pvar = self.next_var();
                self.lines.push(format!(
                    "  let {pv} = twist(p, {:.6});",
                    strength, pv = pvar
                ));
                self.lines.push(format!("  {{ let p = {};", pvar));
                let result = self.emit_node(child);
                self.lines.push("  }".to_string());
                result
            }
            SdfTransform::Bend { strength } => {
                let pvar = self.next_var();
                self.lines.push(format!(
                    "  let {pv} = bend(p, {:.6});",
                    strength, pv = pvar
                ));
                self.lines.push(format!("  {{ let p = {};", pvar));
                let result = self.emit_node(child);
                self.lines.push("  }".to_string());
                result
            }
            SdfTransform::Round { radius } => {
                let inner = self.emit_node(child);
                let var = self.next_var();
                self.lines.push(format!("  let {} = {} - {:.6};", var, inner, radius));
                var
            }
            SdfTransform::Shell { thickness } => {
                let inner = self.emit_node(child);
                let var = self.next_var();
                self.lines.push(format!("  let {} = abs({}) - {:.6};", var, inner, thickness));
                var
            }
            SdfTransform::Elongate { amount } => {
                let pvar = self.next_var();
                self.lines.push(format!(
                    "  let {pv} = p - clamp(p, vec3f(-{:.6}, -{:.6}, -{:.6}), vec3f({:.6}, {:.6}, {:.6}));",
                    amount[0], amount[1], amount[2],
                    amount[0], amount[1], amount[2],
                    pv = pvar
                ));
                self.lines.push(format!("  {{ let p = {};", pvar));
                let result = self.emit_node(child);
                self.lines.push("  }".to_string());
                result
            }
            SdfTransform::Repeat { spacing, count } => {
                let pvar = self.next_var();
                self.lines.push(format!(
                    "  let {pv} = p - clamp(round(p / vec3f({:.6}, {:.6}, {:.6})), vec3f(-{}, -{}, -{}), vec3f({}, {}, {})) * vec3f({:.6}, {:.6}, {:.6});",
                    spacing[0], spacing[1], spacing[2],
                    count[0], count[1], count[2],
                    count[0], count[1], count[2],
                    spacing[0], spacing[1], spacing[2],
                    pv = pvar
                ));
                self.lines.push(format!("  {{ let p = {};", pvar));
                let result = self.emit_node(child);
                self.lines.push("  }".to_string());
                result
            }
            SdfTransform::Mirror { axis } => {
                let pvar = self.next_var();
                self.lines.push(format!(
                    "  let {pv} = abs(p) * vec3f({:.6}, {:.6}, {:.6}) + p * (vec3f(1.0) - vec3f({:.6}, {:.6}, {:.6}));",
                    axis[0].abs(), axis[1].abs(), axis[2].abs(),
                    axis[0].abs(), axis[1].abs(), axis[2].abs(),
                    pv = pvar
                ));
                self.lines.push(format!("  {{ let p = {};", pvar));
                let result = self.emit_node(child);
                self.lines.push("  }".to_string());
                result
            }
            SdfTransform::Displace { frequency, amplitude, octaves } => {
                self.uses_noise = true;
                let inner = self.emit_node(child);
                let var = self.next_var();
                self.lines.push(format!(
                    "  let {} = {} + fbm(p * {:.6}, {}u) * {:.6};",
                    var, inner, frequency, octaves, amplitude
                ));
                var
            }
        }
    }
}

// ─── Shader Template ─────────────────────────────────────────────────

fn build_shader(ctx: &CodegenContext, result_var: &str, settings: &SceneSettings) -> String {
    let mut shader = String::with_capacity(4096);

    // Uniforms
    shader.push_str("struct Uniforms {\n");
    shader.push_str("  resolution: vec2f,\n");
    shader.push_str("  time: f32,\n");
    shader.push_str("  camera_pos: vec3f,\n");
    shader.push_str("  camera_target: vec3f,\n");
    shader.push_str("  fov: f32,\n");
    shader.push_str("};\n\n");
    shader.push_str("@group(0) @binding(0) var<uniform> u: Uniforms;\n");
    shader.push_str("@group(0) @binding(1) var output_texture: texture_storage_2d<rgba8unorm, write>;\n\n");

    // Constants
    shader.push_str(&format!("const MAX_STEPS: u32 = {}u;\n", settings.max_steps));
    shader.push_str(&format!("const MAX_DIST: f32 = {:.1};\n", settings.max_dist));
    shader.push_str(&format!("const EPSILON: f32 = {:.6};\n", settings.epsilon));
    shader.push_str(&format!("const AMBIENT: f32 = {:.4};\n", settings.ambient));
    shader.push_str(&format!(
        "const LIGHT_DIR: vec3f = vec3f({:.6}, {:.6}, {:.6});\n",
        settings.light_dir[0], settings.light_dir[1], settings.light_dir[2]
    ));
    shader.push_str(&format!(
        "const LIGHT_COLOR: vec3f = vec3f({:.4}, {:.4}, {:.4});\n",
        settings.light_color[0], settings.light_color[1], settings.light_color[2]
    ));
    shader.push_str(&format!(
        "const BG_COLOR: vec3f = vec3f({:.4}, {:.4}, {:.4});\n\n",
        settings.background[0], settings.background[1], settings.background[2]
    ));

    // Helper functions
    shader.push_str(HELPER_FUNCTIONS);

    if ctx.uses_smooth_ops {
        shader.push_str(SMOOTH_OPS);
    }
    if ctx.uses_noise {
        shader.push_str(NOISE_FUNCTIONS);
    }

    // SDF scene function
    shader.push_str("fn sdf_scene(p: vec3f) -> f32 {\n");
    for line in &ctx.lines {
        shader.push_str(line);
        shader.push('\n');
    }
    shader.push_str(&format!("  return {};\n", result_var));
    shader.push_str("}\n\n");

    // Normal calculation
    shader.push_str(NORMAL_FUNCTION);

    // Ray marcher
    shader.push_str(RAY_MARCH_FUNCTION);

    // Shading
    if settings.soft_shadows {
        shader.push_str(SOFT_SHADOW_FUNCTION);
    }
    if settings.ao {
        shader.push_str(AO_FUNCTION);
    }
    shader.push_str(SHADE_FUNCTION);

    // Camera
    shader.push_str(CAMERA_FUNCTION);

    // Compute shader entry point
    shader.push_str(COMPUTE_ENTRY);

    shader
}

// ─── WGSL Fragments ──────────────────────────────────────────────────

const HELPER_FUNCTIONS: &str = r#"
fn sdf_box(p: vec3f, b: vec3f) -> f32 {
  let q = abs(p) - b;
  return length(max(q, vec3f(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn sdf_cylinder(p: vec3f, r: f32, h: f32) -> f32 {
  let d = vec2f(length(p.xz) - r, abs(p.y) - h);
  return min(max(d.x, d.y), 0.0) + length(max(d, vec2f(0.0)));
}

fn sdf_torus(p: vec3f, R: f32, r: f32) -> f32 {
  let q = vec2f(length(p.xz) - R, p.y);
  return length(q) - r;
}

fn sdf_capsule(p: vec3f, h: f32, r: f32) -> f32 {
  let py = p.y - clamp(p.y, 0.0, h);
  return length(vec3f(p.x, py, p.z)) - r;
}

fn sdf_cone(p: vec3f, angle: f32, h: f32) -> f32 {
  let c = vec2f(sin(angle), cos(angle));
  let q = vec2f(length(p.xz), p.y);
  let d = length(q - c * max(dot(q, c), 0.0));
  return d * select(1.0, -1.0, q.x * c.y - q.y * c.x < 0.0);
}

fn rot_xyz(p: vec3f, a: vec3f) -> vec3f {
  let cx = cos(a.x); let sx = sin(a.x);
  let cy = cos(a.y); let sy = sin(a.y);
  let cz = cos(a.z); let sz = sin(a.z);
  var q = p;
  q = vec3f(q.x, cx * q.y - sx * q.z, sx * q.y + cx * q.z);
  q = vec3f(cy * q.x + sy * q.z, q.y, -sy * q.x + cy * q.z);
  q = vec3f(cz * q.x - sz * q.y, sz * q.x + cz * q.y, q.z);
  return q;
}

fn twist(p: vec3f, k: f32) -> vec3f {
  let c = cos(k * p.y);
  let s = sin(k * p.y);
  return vec3f(c * p.x - s * p.z, p.y, s * p.x + c * p.z);
}

fn bend(p: vec3f, k: f32) -> vec3f {
  let c = cos(k * p.x);
  let s = sin(k * p.x);
  return vec3f(c * p.x - s * p.y, s * p.x + c * p.y, p.z);
}

"#;

const SMOOTH_OPS: &str = r#"
fn smin(a: f32, b: f32, k: f32) -> f32 {
  let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
  return mix(b, a, h) - k * h * (1.0 - h);
}

fn smax(a: f32, b: f32, k: f32) -> f32 {
  return -smin(-a, -b, k);
}

"#;

const NOISE_FUNCTIONS: &str = r#"
fn hash(p: vec3f) -> f32 {
  var q = fract(p * 0.1031);
  q = q + dot(q, q.yzx + 19.19);
  return fract((q.x + q.y) * q.z);
}

fn noise(p: vec3f) -> f32 {
  let i = floor(p);
  let f = fract(p);
  let u = f * f * (3.0 - 2.0 * f);
  return mix(
    mix(mix(hash(i), hash(i + vec3f(1, 0, 0)), u.x),
        mix(hash(i + vec3f(0, 1, 0)), hash(i + vec3f(1, 1, 0)), u.x), u.y),
    mix(mix(hash(i + vec3f(0, 0, 1)), hash(i + vec3f(1, 0, 1)), u.x),
        mix(hash(i + vec3f(0, 1, 1)), hash(i + vec3f(1, 1, 1)), u.x), u.y), u.z);
}

fn fbm(p: vec3f, octaves: u32) -> f32 {
  var value = 0.0;
  var amplitude = 0.5;
  var q = p;
  for (var i = 0u; i < octaves; i = i + 1u) {
    value = value + amplitude * noise(q);
    q = q * 2.0;
    amplitude = amplitude * 0.5;
  }
  return value;
}

"#;

const NORMAL_FUNCTION: &str = r#"
fn calc_normal(p: vec3f) -> vec3f {
  let e = vec2f(EPSILON, 0.0);
  return normalize(vec3f(
    sdf_scene(p + e.xyy) - sdf_scene(p - e.xyy),
    sdf_scene(p + e.yxy) - sdf_scene(p - e.yxy),
    sdf_scene(p + e.yyx) - sdf_scene(p - e.yyx)
  ));
}

"#;

const RAY_MARCH_FUNCTION: &str = r#"
fn ray_march(ro: vec3f, rd: vec3f) -> f32 {
  var t = 0.0;
  for (var i = 0u; i < MAX_STEPS; i = i + 1u) {
    let p = ro + rd * t;
    let d = sdf_scene(p);
    if d < EPSILON { return t; }
    if t > MAX_DIST { break; }
    t = t + d;
  }
  return -1.0;
}

"#;

const SOFT_SHADOW_FUNCTION: &str = r#"
fn soft_shadow(ro: vec3f, rd: vec3f, mint: f32, maxt: f32, k: f32) -> f32 {
  var res = 1.0;
  var t = mint;
  for (var i = 0u; i < 64u; i = i + 1u) {
    let h = sdf_scene(ro + rd * t);
    if h < EPSILON { return 0.0; }
    res = min(res, k * h / t);
    t = t + clamp(h, 0.01, 0.2);
    if t > maxt { break; }
  }
  return clamp(res, 0.0, 1.0);
}

"#;

const AO_FUNCTION: &str = r#"
fn calc_ao(pos: vec3f, nor: vec3f) -> f32 {
  var occ = 0.0;
  var sca = 1.0;
  for (var i = 0u; i < 5u; i = i + 1u) {
    let h = 0.01 + 0.12 * f32(i);
    let d = sdf_scene(pos + h * nor);
    occ = occ + (h - d) * sca;
    sca = sca * 0.95;
  }
  return clamp(1.0 - 3.0 * occ, 0.0, 1.0);
}

"#;

const SHADE_FUNCTION: &str = r#"
fn shade(ro: vec3f, rd: vec3f, t: f32) -> vec3f {
  let p = ro + rd * t;
  let n = calc_normal(p);

  // Diffuse
  let diff = max(dot(n, LIGHT_DIR), 0.0);

  // Specular (Blinn-Phong)
  let h = normalize(LIGHT_DIR - rd);
  let spec = pow(max(dot(n, h), 0.0), 32.0);

  // Fresnel
  let fresnel = pow(1.0 - max(dot(n, -rd), 0.0), 3.0) * 0.3;

  var col = vec3f(0.8, 0.8, 0.8); // default albedo

  var light = AMBIENT + diff * LIGHT_COLOR + spec * 0.5;

  // AO (if function exists, it's always included when enabled)
  light = light * calc_ao(p, n);

  return col * light + fresnel * LIGHT_COLOR;
}

"#;

const CAMERA_FUNCTION: &str = r#"
fn camera_ray(uv: vec2f, ro: vec3f, target: vec3f, fov: f32) -> vec3f {
  let fwd = normalize(target - ro);
  let right = normalize(cross(vec3f(0.0, 1.0, 0.0), fwd));
  let up = cross(fwd, right);
  let focal = 1.0 / tan(radians(fov) * 0.5);
  return normalize(uv.x * right + uv.y * up + focal * fwd);
}

"#;

const COMPUTE_ENTRY: &str = r#"
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3u) {
  let dims = textureDimensions(output_texture);
  if gid.x >= dims.x || gid.y >= dims.y { return; }

  let uv = (vec2f(f32(gid.x), f32(gid.y)) - vec2f(f32(dims.x), f32(dims.y)) * 0.5) / f32(dims.y);

  let ro = u.camera_pos;
  let rd = camera_ray(uv, ro, u.camera_target, u.fov);

  let t = ray_march(ro, rd);

  var col: vec3f;
  if t > 0.0 {
    col = shade(ro, rd, t);
  } else {
    col = BG_COLOR;
  }

  // Gamma correction
  col = pow(col, vec3f(1.0 / 2.2));

  textureStore(output_texture, vec2i(gid.xy), vec4f(col, 1.0));
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::SdfNode;

    #[test]
    fn test_compile_sphere() {
        let node = SdfNode::sphere(1.0);
        let result = compile(&node);
        assert!(result.wgsl.contains("length(p) - 1.0"));
        assert!(result.wgsl.contains("fn sdf_scene"));
        assert!(result.wgsl.contains("@compute"));
        assert_eq!(result.node_count, 1);
    }

    #[test]
    fn test_compile_smooth_union() {
        let node = SdfNode::smooth_union(
            SdfNode::sphere(1.0).translate([1.0, 0.0, 0.0]),
            SdfNode::cube(0.8),
            0.3,
        );
        let result = compile(&node);
        assert!(result.wgsl.contains("smin("));
        assert!(result.uses_smooth_ops);
        assert!(result.wgsl.contains("fn smin"));
    }

    #[test]
    fn test_compile_with_noise() {
        let node = SdfNode::sphere(1.0).displace(2.0, 0.1, 4);
        let result = compile(&node);
        assert!(result.uses_noise);
        assert!(result.wgsl.contains("fn fbm"));
        assert!(result.wgsl.contains("fn noise"));
    }

    #[test]
    fn test_compile_scene_settings() {
        let node = SdfNode::sphere(1.0).into_scene_with(SceneSettings {
            max_steps: 256,
            soft_shadows: true,
            ..Default::default()
        });
        let result = compile(&node);
        assert!(result.wgsl.contains("MAX_STEPS: u32 = 256u"));
        assert!(result.wgsl.contains("fn soft_shadow"));
    }
}
