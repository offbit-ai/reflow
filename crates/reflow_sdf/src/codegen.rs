//! WGSL code generation from SDF IR trees.
//!
//! Walks the [`SdfNode`] tree and emits a complete WGSL compute shader
//! that ray marches the SDF scene. The output is a standalone shader
//! suitable for `wgpu` or WebGPU.
//!
//! Each node receives a position variable name (initially `"p"`) and
//! transforms create new named positions (e.g. `p1`, `p2`) — no
//! variable shadowing needed.

use crate::ir::*;

/// Compiled shader output.
pub struct CompiledShader {
    /// Complete WGSL source code.
    pub wgsl: String,
    /// Number of SDF nodes compiled.
    pub node_count: u32,
    /// Whether the shader uses noise/displacement.
    pub uses_noise: bool,
    /// Whether the shader uses smooth operations.
    pub uses_smooth_ops: bool,
}

/// Compile an SdfNode tree into a complete WGSL ray marching shader.
pub fn compile(node: &SdfNode) -> CompiledShader {
    let mut ctx = CodegenContext::new();

    let (root, settings) = match node {
        SdfNode::Scene { root, settings } => (root.as_ref(), settings.clone()),
        other => (other, SceneSettings::default()),
    };

    // "p" is the initial position variable from the function parameter
    let sdf_expr = ctx.emit_node(root, "p");

    let wgsl = build_shader(&ctx, &sdf_expr, &settings);

    CompiledShader {
        wgsl,
        node_count: ctx.node_count,
        uses_noise: ctx.uses_noise,
        uses_smooth_ops: ctx.uses_smooth_ops,
    }
}

struct CodegenContext {
    /// Monotonic counter for unique variable names.
    var_counter: u32,
    /// Counter for position variables.
    pos_counter: u32,
    /// Total nodes emitted.
    node_count: u32,
    /// Lines of code in the sdf_scene function body.
    lines: Vec<String>,
    /// Additional helper functions emitted by primitives (e.g. TubePath).
    helpers: Vec<String>,
    uses_noise: bool,
    uses_smooth_ops: bool,
}

impl CodegenContext {
    fn new() -> Self {
        Self {
            var_counter: 0,
            pos_counter: 0,
            node_count: 0,
            lines: Vec::new(),
            helpers: Vec::new(),
            uses_noise: false,
            uses_smooth_ops: false,
        }
    }

    fn next_dist(&mut self) -> String {
        let name = format!("d{}", self.var_counter);
        self.var_counter += 1;
        name
    }

    fn next_pos(&mut self) -> String {
        let name = format!("p{}", self.pos_counter);
        self.pos_counter += 1;
        name
    }

    /// Emit code for a node. `pos` is the name of the position variable
    /// available at this point in the tree. Returns the dist variable name.
    fn emit_node(&mut self, node: &SdfNode, pos: &str) -> String {
        self.node_count += 1;
        match node {
            SdfNode::Primitive { shape, .. } => self.emit_primitive(shape, pos),
            SdfNode::Operation { op, left, right } => {
                let l = self.emit_node(left, pos);
                let r = self.emit_node(right, pos);
                self.emit_op(op, &l, &r)
            }
            SdfNode::Transform { transform, child } => self.emit_transform(transform, child, pos),
            SdfNode::Material { child, .. } => self.emit_node(child, pos),
            SdfNode::Ref { name } => {
                let var = self.next_dist();
                self.lines
                    .push(format!("  let {} = sdf_{}({});", var, name, pos));
                var
            }
            SdfNode::Scene { root, .. } => self.emit_node(root, pos),
        }
    }

    fn emit_primitive(&mut self, shape: &SdfPrimitive, pos: &str) -> String {
        let var = self.next_dist();
        let expr = match shape {
            SdfPrimitive::Sphere { radius } =>
                format!("length({}) - {:.6}", pos, radius),
            SdfPrimitive::Box { size } =>
                format!("sdf_box({}, vec3f({:.6}, {:.6}, {:.6}))", pos, size[0], size[1], size[2]),
            SdfPrimitive::RoundBox { size, radius } =>
                format!("sdf_box({}, vec3f({:.6}, {:.6}, {:.6})) - {:.6}", pos, size[0], size[1], size[2], radius),
            SdfPrimitive::Cylinder { radius, height } =>
                format!("sdf_cylinder({}, {:.6}, {:.6})", pos, radius, height),
            SdfPrimitive::Capsule { radius, height } =>
                format!("sdf_capsule({}, {:.6}, {:.6})", pos, height, radius),
            SdfPrimitive::Torus { major_radius, minor_radius } =>
                format!("sdf_torus({}, {:.6}, {:.6})", pos, major_radius, minor_radius),
            SdfPrimitive::Cone { angle, height } =>
                format!("sdf_cone({}, {:.6}, {:.6})", pos, angle, height),
            SdfPrimitive::TaperedCapsule { a, b, radius_a, radius_b } =>
                format!("sdf_tapered_capsule({}, vec3f({:.6}, {:.6}, {:.6}), vec3f({:.6}, {:.6}, {:.6}), {:.6}, {:.6})",
                    pos, a[0], a[1], a[2], b[0], b[1], b[2], radius_a, radius_b),
            SdfPrimitive::TubePath { points, radii, k: _ } => {
                // Emit a function with constant arrays + loop for the tubular path.
                // Uses closest-point on polyline with arc-length radius interpolation.
                let n = points.len().min(radii.len());
                if n < 2 {
                    "999.0".to_string()
                } else {
                    let fn_name = format!("sdf_tube_{}", var);
                    let mut func = String::new();

                    // Emit constant arrays
                    func.push_str(&format!("const TUBE_{}_N: u32 = {}u;\n", var, n));
                    func.push_str(&format!("const TUBE_{}_PTS: array<vec3f, {}> = array<vec3f, {}>(\n", var, n, n));
                    for (i, p) in points.iter().take(n).enumerate() {
                        func.push_str(&format!("  vec3f({:.6}, {:.6}, {:.6}){}\n",
                            p[0], p[1], p[2], if i < n - 1 { "," } else { "" }));
                    }
                    func.push_str(");\n");
                    func.push_str(&format!("const TUBE_{}_RAD: array<f32, {}> = array<f32, {}>(\n", var, n, n));
                    for (i, r) in radii.iter().take(n).enumerate() {
                        func.push_str(&format!("  {:.6}{}\n", r, if i < n - 1 { "," } else { "" }));
                    }
                    func.push_str(");\n");

                    // Precompute cumulative arc lengths for global t parameterization
                    let mut arc = vec![0.0f32; n];
                    for ii in 1..n {
                        let dx = points[ii][0] - points[ii-1][0];
                        let dy = points[ii][1] - points[ii-1][1];
                        let dz = points[ii][2] - points[ii-1][2];
                        arc[ii] = arc[ii-1] + (dx*dx + dy*dy + dz*dz).sqrt();
                    }
                    let total_arc = if arc[n-1] > 0.0 { arc[n-1] } else { 1.0 };

                    // Emit arc-length lookup table
                    func.push_str(&format!("const TUBE_{}_ARC: array<f32, {}> = array<f32, {}>(\n", var, n, n));
                    for (ii, a) in arc.iter().enumerate() {
                        func.push_str(&format!("  {:.8}{}\n", a / total_arc, if ii < n - 1 { "," } else { "" }));
                    }
                    func.push_str(");\n");

                    // Emit function: hard closest-point with arc-length radius.
                    // Distance is C0 at Voronoi boundaries but radius is continuous
                    // via global arc-length parameterization.
                    func.push_str(&format!("fn {}(p: vec3f) -> f32 {{\n", fn_name));
                    func.push_str(&format!("  let N = TUBE_{}_N;\n", var));
                    func.push_str("  var best_d2 = 1e10;\n");
                    func.push_str("  var best_gt = 0.0;\n");
                    func.push_str("  for (var i = 0u; i < N - 1u; i = i + 1u) {\n");
                    func.push_str(&format!("    let a = TUBE_{}_PTS[i];\n", var));
                    func.push_str(&format!("    let b = TUBE_{}_PTS[i + 1u];\n", var));
                    func.push_str("    let ba = b - a;\n");
                    func.push_str("    let pa = p - a;\n");
                    func.push_str("    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);\n");
                    func.push_str("    let closest = pa - ba * h;\n");
                    func.push_str("    let d2 = dot(closest, closest);\n");
                    func.push_str(&format!("    let gt = mix(TUBE_{}_ARC[i], TUBE_{}_ARC[i + 1u], h);\n", var, var));
                    func.push_str("    if d2 < best_d2 { best_d2 = d2; best_gt = gt; }\n");
                    func.push_str("  }\n");
                    // Radius from global arc-length
                    func.push_str(&format!("  var r = TUBE_{}_RAD[N - 1u];\n", var));
                    func.push_str("  for (var i = 0u; i < N - 1u; i = i + 1u) {\n");
                    func.push_str(&format!("    if best_gt <= TUBE_{}_ARC[i + 1u] {{\n", var));
                    func.push_str(&format!("      let t0 = TUBE_{}_ARC[i];\n", var));
                    func.push_str(&format!("      let t1 = TUBE_{}_ARC[i + 1u];\n", var));
                    func.push_str("      let lt = (best_gt - t0) / max(t1 - t0, 0.0001);\n");
                    func.push_str(&format!("      r = mix(TUBE_{}_RAD[i], TUBE_{}_RAD[i + 1u], lt);\n", var, var));
                    func.push_str("      break;\n");
                    func.push_str("    }\n");
                    func.push_str("  }\n");
                    func.push_str("  return sqrt(best_d2) - r;\n}\n");

                    self.helpers.push(func);
                    format!("{}({})", fn_name, pos)
                }
            }
            SdfPrimitive::Plane { normal, offset } =>
                format!("dot({}, vec3f({:.6}, {:.6}, {:.6})) + {:.6}", pos, normal[0], normal[1], normal[2], offset),
            SdfPrimitive::InfRepeat { spacing } =>
                format!("length({p} - round({p} / vec3f({:.6}, {:.6}, {:.6})) * vec3f({:.6}, {:.6}, {:.6}))",
                    spacing[0], spacing[1], spacing[2], spacing[0], spacing[1], spacing[2], p = pos),
        };
        self.lines.push(format!("  let {} = {};", var, expr));
        var
    }

    fn emit_op(&mut self, op: &SdfOp, left: &str, right: &str) -> String {
        let var = self.next_dist();
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

    fn emit_transform(&mut self, transform: &SdfTransform, child: &SdfNode, pos: &str) -> String {
        match transform {
            SdfTransform::Translate { offset } => {
                let new_pos = self.next_pos();
                self.lines.push(format!(
                    "  let {} = {} - vec3f({:.6}, {:.6}, {:.6});",
                    new_pos, pos, offset[0], offset[1], offset[2]
                ));
                self.emit_node(child, &new_pos)
            }
            SdfTransform::Rotate { angles } => {
                let new_pos = self.next_pos();
                self.lines.push(format!(
                    "  let {} = rot_xyz({}, vec3f({:.6}, {:.6}, {:.6}));",
                    new_pos,
                    pos,
                    angles[0].to_radians(),
                    angles[1].to_radians(),
                    angles[2].to_radians()
                ));
                self.emit_node(child, &new_pos)
            }
            SdfTransform::Scale { factor } => {
                let s = factor[0]; // uniform scale
                let new_pos = self.next_pos();
                self.lines
                    .push(format!("  let {} = {} / {:.6};", new_pos, pos, s));
                let inner = self.emit_node(child, &new_pos);
                let var = self.next_dist();
                self.lines
                    .push(format!("  let {} = {} * {:.6};", var, inner, s));
                var
            }
            SdfTransform::Twist { strength } => {
                let new_pos = self.next_pos();
                self.lines.push(format!(
                    "  let {} = twist({}, {:.6});",
                    new_pos, pos, strength
                ));
                self.emit_node(child, &new_pos)
            }
            SdfTransform::Bend { strength } => {
                let new_pos = self.next_pos();
                self.lines.push(format!(
                    "  let {} = bend({}, {:.6});",
                    new_pos, pos, strength
                ));
                self.emit_node(child, &new_pos)
            }
            SdfTransform::Elongate { amount } => {
                let new_pos = self.next_pos();
                self.lines.push(format!(
                    "  let {} = {} - clamp({}, vec3f(-{:.6}, -{:.6}, -{:.6}), vec3f({:.6}, {:.6}, {:.6}));",
                    new_pos, pos, pos,
                    amount[0], amount[1], amount[2],
                    amount[0], amount[1], amount[2]
                ));
                self.emit_node(child, &new_pos)
            }
            SdfTransform::Round { radius } => {
                let inner = self.emit_node(child, pos);
                let var = self.next_dist();
                self.lines
                    .push(format!("  let {} = {} - {:.6};", var, inner, radius));
                var
            }
            SdfTransform::Shell { thickness } => {
                let inner = self.emit_node(child, pos);
                let var = self.next_dist();
                self.lines.push(format!(
                    "  let {} = abs({}) - {:.6};",
                    var, inner, thickness
                ));
                var
            }
            SdfTransform::Repeat { spacing, count } => {
                let new_pos = self.next_pos();
                self.lines.push(format!(
                    "  let {} = {} - clamp(round({} / vec3f({:.6}, {:.6}, {:.6})), vec3f(-{}.0, -{}.0, -{}.0), vec3f({}.0, {}.0, {}.0)) * vec3f({:.6}, {:.6}, {:.6});",
                    new_pos, pos, pos,
                    spacing[0], spacing[1], spacing[2],
                    count[0], count[1], count[2],
                    count[0], count[1], count[2],
                    spacing[0], spacing[1], spacing[2]
                ));
                self.emit_node(child, &new_pos)
            }
            SdfTransform::Mirror { axis } => {
                let new_pos = self.next_pos();
                // For each axis with value > 0, take abs of that component
                let mut components = Vec::new();
                for (i, &a) in axis.iter().enumerate() {
                    let comp = ["x", "y", "z"][i];
                    if a.abs() > 0.5 {
                        components.push(format!("abs({}.{})", pos, comp));
                    } else {
                        components.push(format!("{}.{}", pos, comp));
                    }
                }
                self.lines.push(format!(
                    "  let {} = vec3f({}, {}, {});",
                    new_pos, components[0], components[1], components[2]
                ));
                self.emit_node(child, &new_pos)
            }
            SdfTransform::Displace {
                frequency,
                amplitude,
                octaves,
            } => {
                self.uses_noise = true;
                let inner = self.emit_node(child, pos);
                let var = self.next_dist();
                self.lines.push(format!(
                    "  let {} = {} + fbm({} * {:.6}, {}u) * {:.6};",
                    var, inner, pos, frequency, octaves, amplitude
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
    shader.push_str("  _pad0: f32,\n");
    shader.push_str("  camera_pos: vec3f,\n");
    shader.push_str("  _pad1: f32,\n");
    shader.push_str("  camera_target: vec3f,\n");
    shader.push_str("  fov: f32,\n");
    shader.push_str("};\n\n");
    shader.push_str("@group(0) @binding(0) var<uniform> u: Uniforms;\n");
    shader.push_str(
        "@group(0) @binding(1) var output_texture: texture_storage_2d<rgba8unorm, write>;\n\n",
    );

    // Constants
    shader.push_str(&format!(
        "const MAX_STEPS: u32 = {}u;\n",
        settings.max_steps
    ));
    shader.push_str(&format!(
        "const MAX_DIST: f32 = {:.1};\n",
        settings.max_dist
    ));
    shader.push_str(&format!("const EPSILON: f32 = {:.6};\n", settings.epsilon));
    shader.push_str(&format!("const AMBIENT: f32 = {:.4};\n", settings.ambient));
    // Pre-normalize the light direction (normalize() not available in const)
    let ld = settings.light_dir;
    let ld_len = (ld[0] * ld[0] + ld[1] * ld[1] + ld[2] * ld[2]).sqrt();
    let ld_n = if ld_len > 0.0 {
        [ld[0] / ld_len, ld[1] / ld_len, ld[2] / ld_len]
    } else {
        ld
    };
    shader.push_str(&format!(
        "const LIGHT_DIR: vec3f = vec3f({:.6}, {:.6}, {:.6});\n",
        ld_n[0], ld_n[1], ld_n[2]
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

    // Emit helper functions (TubePath, etc.)
    for helper in &ctx.helpers {
        shader.push_str(helper);
        shader.push('\n');
    }

    // SDF scene function
    shader.push_str("fn sdf_scene(p: vec3f) -> f32 {\n");
    for line in &ctx.lines {
        shader.push_str(line);
        shader.push('\n');
    }
    shader.push_str(&format!("  return {};\n", result_var));
    shader.push_str("}\n\n");

    // Rendering functions
    shader.push_str(NORMAL_FUNCTION);
    shader.push_str(RAY_MARCH_FUNCTION);
    if settings.soft_shadows {
        shader.push_str(SOFT_SHADOW_FUNCTION);
    }
    if settings.ao {
        shader.push_str(AO_FUNCTION);
    }
    shader.push_str(if settings.ao {
        SHADE_WITH_AO
    } else {
        SHADE_NO_AO
    });
    shader.push_str(CAMERA_FUNCTION);
    shader.push_str(COMPUTE_ENTRY);

    shader
}

// ─── WGSL Fragments ──────────────────────────────────────────────────

const HELPER_FUNCTIONS: &str = r#"fn sdf_box(p: vec3f, b: vec3f) -> f32 {
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
  var q = p;
  q.y = q.y - clamp(q.y, 0.0, h);
  return length(q) - r;
}

fn sdf_cone(p: vec3f, angle: f32, h: f32) -> f32 {
  let c = vec2f(sin(angle), cos(angle));
  let q = vec2f(length(p.xz), p.y);
  let d = length(q - c * max(dot(q, c), 0.0));
  return d * select(1.0, -1.0, q.x * c.y - q.y * c.x < 0.0);
}

fn sdf_tapered_capsule(p: vec3f, a: vec3f, b: vec3f, ra: f32, rb: f32) -> f32 {
  let ba = b - a;
  let pa = p - a;
  let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
  return length(pa - ba * h) - mix(ra, rb, h);
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

const SMOOTH_OPS: &str = r#"fn smin(a: f32, b: f32, k: f32) -> f32 {
  let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
  return mix(b, a, h) - k * h * (1.0 - h);
}

fn smax(a: f32, b: f32, k: f32) -> f32 {
  return -smin(-a, -b, k);
}

"#;

const NOISE_FUNCTIONS: &str = r#"fn hash3(p: vec3f) -> f32 {
  var q = fract(p * 0.1031);
  q = q + dot(q, q.yzx + 19.19);
  return fract((q.x + q.y) * q.z);
}

fn noise3d(p: vec3f) -> f32 {
  let i = floor(p);
  let f = fract(p);
  let u = f * f * (3.0 - 2.0 * f);
  return mix(
    mix(mix(hash3(i), hash3(i + vec3f(1.0, 0.0, 0.0)), u.x),
        mix(hash3(i + vec3f(0.0, 1.0, 0.0)), hash3(i + vec3f(1.0, 1.0, 0.0)), u.x), u.y),
    mix(mix(hash3(i + vec3f(0.0, 0.0, 1.0)), hash3(i + vec3f(1.0, 0.0, 1.0)), u.x),
        mix(hash3(i + vec3f(0.0, 1.0, 1.0)), hash3(i + vec3f(1.0, 1.0, 1.0)), u.x), u.y), u.z);
}

fn fbm(p: vec3f, octaves: u32) -> f32 {
  var value = 0.0;
  var amplitude = 0.5;
  var q = p;
  for (var i = 0u; i < octaves; i = i + 1u) {
    value = value + amplitude * noise3d(q);
    q = q * 2.0;
    amplitude = amplitude * 0.5;
  }
  return value;
}

"#;

const NORMAL_FUNCTION: &str = r#"fn calc_normal(p: vec3f) -> vec3f {
  let e = vec2f(EPSILON, 0.0);
  return normalize(vec3f(
    sdf_scene(p + e.xyy) - sdf_scene(p - e.xyy),
    sdf_scene(p + e.yxy) - sdf_scene(p - e.yxy),
    sdf_scene(p + e.yyx) - sdf_scene(p - e.yyx)
  ));
}

"#;

const RAY_MARCH_FUNCTION: &str = r#"fn ray_march(ro: vec3f, rd: vec3f) -> f32 {
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

const SOFT_SHADOW_FUNCTION: &str = r#"fn soft_shadow(ro: vec3f, rd: vec3f, mint: f32, maxt: f32, k: f32) -> f32 {
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

const AO_FUNCTION: &str = r#"fn calc_ao(pos: vec3f, nor: vec3f) -> f32 {
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

const SHADE_WITH_AO: &str = r#"fn shade(ro: vec3f, rd: vec3f, t: f32) -> vec3f {
  let p = ro + rd * t;
  let n = calc_normal(p);
  let diff = max(dot(n, LIGHT_DIR), 0.0);
  let h = normalize(LIGHT_DIR - rd);
  let spec = pow(max(dot(n, h), 0.0), 32.0);
  let fresnel = pow(1.0 - max(dot(n, -rd), 0.0), 3.0) * 0.3;
  let ao = calc_ao(p, n);
  let col = vec3f(0.8, 0.8, 0.8);
  let light = (AMBIENT + diff * LIGHT_COLOR + spec * 0.5) * ao;
  return col * light + fresnel * LIGHT_COLOR;
}

"#;

const SHADE_NO_AO: &str = r#"fn shade(ro: vec3f, rd: vec3f, t: f32) -> vec3f {
  let p = ro + rd * t;
  let n = calc_normal(p);
  let diff = max(dot(n, LIGHT_DIR), 0.0);
  let h = normalize(LIGHT_DIR - rd);
  let spec = pow(max(dot(n, h), 0.0), 32.0);
  let fresnel = pow(1.0 - max(dot(n, -rd), 0.0), 3.0) * 0.3;
  let col = vec3f(0.8, 0.8, 0.8);
  let light = AMBIENT + diff * LIGHT_COLOR + spec * 0.5;
  return col * light + fresnel * LIGHT_COLOR;
}

"#;

const CAMERA_FUNCTION: &str = r#"fn camera_ray(uv: vec2f, ro: vec3f, look_at: vec3f, fov_deg: f32) -> vec3f {
  let fwd = normalize(look_at - ro);
  let right = normalize(cross(vec3f(0.0, 1.0, 0.0), fwd));
  let up = cross(fwd, right);
  let focal = 1.0 / tan(radians(fov_deg) * 0.5);
  return normalize(uv.x * right + uv.y * up + focal * fwd);
}

"#;

const COMPUTE_ENTRY: &str = r#"@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3u) {
  let dims = textureDimensions(output_texture);
  if gid.x >= dims.x || gid.y >= dims.y { return; }

  let uv = (vec2f(f32(gid.x), f32(dims.y) - f32(gid.y)) - vec2f(f32(dims.x), f32(dims.y)) * 0.5) / f32(dims.y);

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
        let result = compile(&SdfNode::sphere(1.0));
        assert!(result.wgsl.contains("let d0 = length(p) - 1.0"));
        assert!(result.wgsl.contains("return d0;"));
        assert_eq!(result.node_count, 1);
    }

    #[test]
    fn test_compile_translated_sphere() {
        let result = compile(&SdfNode::sphere(1.0).translate([2.0, 0.0, 0.0]));
        // Should create p0 = p - translate, then use p0 for sphere
        assert!(result.wgsl.contains("let p0 = p - vec3f(2.0"));
        assert!(result.wgsl.contains("length(p0) - 1.0"));
    }

    #[test]
    fn test_compile_nested_transforms() {
        let result = compile(
            &SdfNode::sphere(1.0)
                .translate([1.0, 0.0, 0.0])
                .rotate([0.0, 45.0, 0.0]),
        );
        // Should create p0 for rotate, p1 for translate, each with unique names
        assert!(result.wgsl.contains("let p0 = rot_xyz(p,"));
        assert!(result.wgsl.contains("let p1 = p0 - vec3f(1.0"));
        assert!(result.wgsl.contains("length(p1)"));
    }

    #[test]
    fn test_compile_smooth_union() {
        let result = compile(&SdfNode::smooth_union(
            SdfNode::sphere(1.0).translate([1.0, 0.0, 0.0]),
            SdfNode::cube(0.8),
            0.3,
        ));
        assert!(result.uses_smooth_ops);
        assert!(result.wgsl.contains("fn smin"));
        assert!(result.wgsl.contains("smin("));
    }

    #[test]
    fn test_compile_displacement() {
        let result = compile(&SdfNode::sphere(1.0).displace(2.0, 0.1, 4));
        assert!(result.uses_noise);
        assert!(result.wgsl.contains("fn fbm"));
        assert!(result.wgsl.contains("fbm(p * 2.0"));
    }

    #[test]
    fn test_compile_scene_settings() {
        let result = compile(&SdfNode::sphere(1.0).into_scene_with(SceneSettings {
            max_steps: 256,
            soft_shadows: true,
            ao: true,
            ..Default::default()
        }));
        assert!(result.wgsl.contains("MAX_STEPS: u32 = 256u"));
        assert!(result.wgsl.contains("fn soft_shadow"));
        assert!(result.wgsl.contains("fn calc_ao"));
    }

    #[test]
    fn test_no_variable_shadowing() {
        // Ensure no `let p = ...` appears (only p0, p1, etc.)
        let result = compile(&SdfNode::smooth_union(
            SdfNode::sphere(0.5).translate([1.0, 0.0, 0.0]).twist(0.3),
            SdfNode::torus(1.0, 0.3).rotate([90.0, 0.0, 0.0]),
            0.2,
        ));
        // The function parameter is `p`, all derived positions are p0, p1, ...
        // We should NOT see `let p =` (which would shadow the parameter)
        let sdf_fn_body = result
            .wgsl
            .split("fn sdf_scene(p: vec3f) -> f32 {")
            .nth(1)
            .unwrap();
        let sdf_fn_body = sdf_fn_body.split('}').next().unwrap();
        assert!(
            !sdf_fn_body.contains("let p ="),
            "Should not shadow p: {}",
            sdf_fn_body
        );
    }

    #[test]
    fn test_uniform_alignment() {
        // Verify uniform struct has proper padding for WebGPU alignment
        let result = compile(&SdfNode::sphere(1.0));
        assert!(result.wgsl.contains("_pad0: f32"));
        assert!(result.wgsl.contains("_pad1: f32"));
    }

    #[test]
    fn test_mirror() {
        let result = compile(&SdfNode::sphere(1.0).mirror([1.0, 0.0, 0.0]));
        assert!(result.wgsl.contains("abs(p.x)"));
        assert!(result.wgsl.contains("p.y"));
    }

    #[test]
    fn test_shell() {
        let result = compile(&SdfNode::sphere(1.0).shell(0.05));
        assert!(result.wgsl.contains("abs(d0) - 0.05"));
    }
}
