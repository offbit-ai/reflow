//! Marching Cubes lookup tables and WGSL shader generation.
//!
//! The edge table and triangle table encode all 256 cube configurations.
//! These are embedded into the WGSL compute shader as storage buffers.

/// Edge table: for each of 256 cube configurations, a 12-bit mask of which edges are intersected.
pub const EDGE_TABLE: [u32; 256] = [
    0x000, 0x109, 0x203, 0x30a, 0x406, 0x50f, 0x605, 0x70c,
    0x80c, 0x905, 0xa0f, 0xb06, 0xc0a, 0xd03, 0xe09, 0xf00,
    0x190, 0x099, 0x393, 0x29a, 0x596, 0x49f, 0x795, 0x69c,
    0x99c, 0x895, 0xb9f, 0xa96, 0xd9a, 0xc93, 0xf99, 0xe90,
    0x230, 0x339, 0x033, 0x13a, 0x636, 0x73f, 0x435, 0x53c,
    0xa3c, 0xb35, 0x83f, 0x936, 0xe3a, 0xf33, 0xc39, 0xd30,
    0x3a0, 0x2a9, 0x1a3, 0x0aa, 0x7a6, 0x6af, 0x5a5, 0x4ac,
    0xbac, 0xaa5, 0x9af, 0x8a6, 0xfaa, 0xea3, 0xda9, 0xca0,
    0x460, 0x569, 0x663, 0x76a, 0x066, 0x16f, 0x265, 0x36c,
    0xc6c, 0xd65, 0xe6f, 0xf66, 0x86a, 0x963, 0xa69, 0xb60,
    0x5f0, 0x4f9, 0x7f3, 0x6fa, 0x1f6, 0x0ff, 0x3f5, 0x2fc,
    0xdfc, 0xcf5, 0xfff, 0xef6, 0x9fa, 0x8f3, 0xbf9, 0xaf0,
    0x650, 0x759, 0x453, 0x55a, 0x256, 0x35f, 0x055, 0x15c,
    0xe5c, 0xf55, 0xc5f, 0xd56, 0xa5a, 0xb53, 0x859, 0x950,
    0x7c0, 0x6c9, 0x5c3, 0x4ca, 0x3c6, 0x2cf, 0x1c5, 0x0cc,
    0xfcc, 0xec5, 0xdcf, 0xcc6, 0xbca, 0xac3, 0x9c9, 0x8c0,
    0x8c0, 0x9c9, 0xac3, 0xbca, 0xcc6, 0xdcf, 0xec5, 0xfcc,
    0x0cc, 0x1c5, 0x2cf, 0x3c6, 0x4ca, 0x5c3, 0x6c9, 0x7c0,
    0x950, 0x859, 0xb53, 0xa5a, 0xd56, 0xc5f, 0xf55, 0xe5c,
    0x15c, 0x055, 0x35f, 0x256, 0x55a, 0x453, 0x759, 0x650,
    0xaf0, 0xbf9, 0x8f3, 0x9fa, 0xef6, 0xfff, 0xcf5, 0xdfc,
    0x2fc, 0x3f5, 0x0ff, 0x1f6, 0x6fa, 0x7f3, 0x4f9, 0x5f0,
    0xb60, 0xa69, 0x963, 0x86a, 0xf66, 0xe6f, 0xd65, 0xc6c,
    0x36c, 0x265, 0x16f, 0x066, 0x76a, 0x663, 0x569, 0x460,
    0xca0, 0xda9, 0xea3, 0xfaa, 0x8a6, 0x9af, 0xaa5, 0xbac,
    0x4ac, 0x5a5, 0x6af, 0x7a6, 0x0aa, 0x1a3, 0x2a9, 0x3a0,
    0xd30, 0xc39, 0xf33, 0xe3a, 0x936, 0x83f, 0xb35, 0xa3c,
    0x53c, 0x435, 0x73f, 0x636, 0x13a, 0x033, 0x339, 0x230,
    0xe90, 0xf99, 0xc93, 0xd9a, 0xa96, 0xb9f, 0x895, 0x99c,
    0x69c, 0x795, 0x49f, 0x596, 0x29a, 0x393, 0x099, 0x190,
    0xf00, 0xe09, 0xd03, 0xc0a, 0xb06, 0xa0f, 0x905, 0x80c,
    0x70c, 0x605, 0x50f, 0x406, 0x30a, 0x203, 0x109, 0x000,
];

/// Triangle table: for each of 256 configs, up to 5 triangles (15 edge indices), terminated by -1.
/// Stored flat: 256 × 16 = 4096 entries.
pub const TRI_TABLE: [i32; 4096] = include!("mc_tri_table.inc");

/// Generate WGSL code for the marching cubes compute shader.
///
/// The shader evaluates `sdf_scene(p)` from the compiled SDF, then runs
/// marching cubes on a 3D grid. Output is an atomic counter + vertex buffer.
pub fn generate_marching_cubes_wgsl(sdf_wgsl: &str, resolution: u32) -> String {
    let mut shader = String::with_capacity(sdf_wgsl.len() + 8192);

    // Uniforms
    shader.push_str("struct MCUniforms {\n");
    shader.push_str("  resolution: u32,\n");
    shader.push_str("  bound_min: vec3f,\n");
    shader.push_str("  bound_max: vec3f,\n");
    shader.push_str("  iso_level: f32,\n");
    shader.push_str("};\n\n");

    shader.push_str("@group(0) @binding(0) var<uniform> mc: MCUniforms;\n");
    shader.push_str("@group(0) @binding(1) var<storage, read> edge_table: array<u32, 256>;\n");
    shader.push_str("@group(0) @binding(2) var<storage, read> tri_table: array<i32, 4096>;\n");
    shader.push_str("@group(0) @binding(3) var<storage, read_write> vertices: array<f32>;\n");
    shader.push_str("@group(0) @binding(4) var<storage, read_write> counter: atomic<u32>;\n\n");

    // Include the SDF helper functions and sdf_scene from the compiled shader
    // Extract everything between the helpers and the ray marching code
    if let Some(sdf_start) = sdf_wgsl.find("fn sdf_box(") {
        if let Some(sdf_end) = sdf_wgsl.find("fn calc_normal(") {
            shader.push_str(&sdf_wgsl[sdf_start..sdf_end]);
        }
    }

    // Marching cubes vertex interpolation
    shader.push_str(r#"
fn mc_interp(p1: vec3f, p2: vec3f, v1: f32, v2: f32, iso: f32) -> vec3f {
  if abs(iso - v1) < 0.00001 { return p1; }
  if abs(iso - v2) < 0.00001 { return p2; }
  if abs(v1 - v2) < 0.00001 { return p1; }
  let mu = (iso - v1) / (v2 - v1);
  return p1 + mu * (p2 - p1);
}

fn mc_normal(p: vec3f) -> vec3f {
  let e = 0.001;
  return normalize(vec3f(
    sdf_scene(p + vec3f(e, 0.0, 0.0)) - sdf_scene(p - vec3f(e, 0.0, 0.0)),
    sdf_scene(p + vec3f(0.0, e, 0.0)) - sdf_scene(p - vec3f(0.0, e, 0.0)),
    sdf_scene(p + vec3f(0.0, 0.0, e)) - sdf_scene(p - vec3f(0.0, 0.0, e))
  ));
}

fn emit_vertex(pos: vec3f, nor: vec3f) {
  let idx = atomicAdd(&counter, 6u);
  vertices[idx]     = pos.x;
  vertices[idx + 1u] = pos.y;
  vertices[idx + 2u] = pos.z;
  vertices[idx + 3u] = nor.x;
  vertices[idx + 4u] = nor.y;
  vertices[idx + 5u] = nor.z;
}

"#);

    // Compute entry point
    shader.push_str(&format!(r#"
@compute @workgroup_size(4, 4, 4)
fn main(@builtin(global_invocation_id) gid: vec3u) {{
  let res = mc.resolution;
  if gid.x >= res - 1u || gid.y >= res - 1u || gid.z >= res - 1u {{ return; }}

  let step = (mc.bound_max - mc.bound_min) / f32(res);
  let base = mc.bound_min + vec3f(f32(gid.x), f32(gid.y), f32(gid.z)) * step;

  // 8 corners of the cube
  let p0 = base;
  let p1 = base + vec3f(step.x, 0.0, 0.0);
  let p2 = base + vec3f(step.x, step.y, 0.0);
  let p3 = base + vec3f(0.0, step.y, 0.0);
  let p4 = base + vec3f(0.0, 0.0, step.z);
  let p5 = base + vec3f(step.x, 0.0, step.z);
  let p6 = base + step;
  let p7 = base + vec3f(0.0, step.y, step.z);

  let v0 = sdf_scene(p0);
  let v1 = sdf_scene(p1);
  let v2 = sdf_scene(p2);
  let v3 = sdf_scene(p3);
  let v4 = sdf_scene(p4);
  let v5 = sdf_scene(p5);
  let v6 = sdf_scene(p6);
  let v7 = sdf_scene(p7);

  var cube_index = 0u;
  let iso = mc.iso_level;
  if v0 < iso {{ cube_index |= 1u; }}
  if v1 < iso {{ cube_index |= 2u; }}
  if v2 < iso {{ cube_index |= 4u; }}
  if v3 < iso {{ cube_index |= 8u; }}
  if v4 < iso {{ cube_index |= 16u; }}
  if v5 < iso {{ cube_index |= 32u; }}
  if v6 < iso {{ cube_index |= 64u; }}
  if v7 < iso {{ cube_index |= 128u; }}

  let edges = edge_table[cube_index];
  if edges == 0u {{ return; }}

  // Interpolate edge vertices
  var vert_list: array<vec3f, 12>;
  if (edges & 1u) != 0u    {{ vert_list[0]  = mc_interp(p0, p1, v0, v1, iso); }}
  if (edges & 2u) != 0u    {{ vert_list[1]  = mc_interp(p1, p2, v1, v2, iso); }}
  if (edges & 4u) != 0u    {{ vert_list[2]  = mc_interp(p2, p3, v2, v3, iso); }}
  if (edges & 8u) != 0u    {{ vert_list[3]  = mc_interp(p3, p0, v3, v0, iso); }}
  if (edges & 16u) != 0u   {{ vert_list[4]  = mc_interp(p4, p5, v4, v5, iso); }}
  if (edges & 32u) != 0u   {{ vert_list[5]  = mc_interp(p5, p6, v5, v6, iso); }}
  if (edges & 64u) != 0u   {{ vert_list[6]  = mc_interp(p6, p7, v6, v7, iso); }}
  if (edges & 128u) != 0u  {{ vert_list[7]  = mc_interp(p7, p4, v7, v4, iso); }}
  if (edges & 256u) != 0u  {{ vert_list[8]  = mc_interp(p0, p4, v0, v4, iso); }}
  if (edges & 512u) != 0u  {{ vert_list[9]  = mc_interp(p1, p5, v1, v5, iso); }}
  if (edges & 1024u) != 0u {{ vert_list[10] = mc_interp(p2, p6, v2, v6, iso); }}
  if (edges & 2048u) != 0u {{ vert_list[11] = mc_interp(p3, p7, v3, v7, iso); }}

  // Generate triangles
  let tri_base = cube_index * 16u;
  for (var i = 0u; i < 15u; i = i + 3u) {{
    let e0 = tri_table[tri_base + i];
    if e0 < 0 {{ break; }}
    let e1 = tri_table[tri_base + i + 1u];
    let e2 = tri_table[tri_base + i + 2u];

    let va = vert_list[e0];
    let vb = vert_list[e1];
    let vc = vert_list[e2];

    let na = mc_normal(va);
    let nb = mc_normal(vb);
    let nc = mc_normal(vc);

    emit_vertex(va, na);
    emit_vertex(vb, nb);
    emit_vertex(vc, nc);
  }}
}}
"#));

    shader
}
