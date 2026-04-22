//! Shadow Map — renders depth-only pass from light perspective.
//!
//! For each shadow-casting directional light, renders the scene from
//! the light's viewpoint into a depth texture. The depth texture is
//! output as bytes for the PBR shader to sample during the main pass.
//!
//! ## Config
//! - `resolution` — shadow map resolution (default 1024)
//! - `bias` — depth bias to prevent shadow acne (default 0.005)
//! - `orthoSize` — orthographic frustum half-size for directional lights (default 500)
//! - `near` / `far` — near/far planes (default 1.0 / 2000.0)
//!
//! ## Inports
//! - `meshes` — mesh vertex data (same as scene render)
//! - `lights` — packed light buffer from LightCollectorActor
//! - `light_count` — number of lights
//!
//! ## Outports
//! - `shadow_map` — depth texture as f32 LE bytes (resolution × resolution)
//! - `shadow_matrix` — light view-projection matrix as f32 LE bytes (64 bytes)
//! - `metadata` — JSON { resolution, bias, lightIndex }

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    ShadowMapActor,
    inports::<10>(meshes, lights, light_count),
    outports::<1>(shadow_map, shadow_matrix, metadata),
    state(MemoryState),
    await_inports(meshes)
)]
pub async fn shadow_map_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let resolution = config
        .get("resolution")
        .and_then(|v| v.as_u64())
        .unwrap_or(1024) as u32;
    let bias = config.get("bias").and_then(|v| v.as_f64()).unwrap_or(0.005) as f32;
    let ortho_size = config
        .get("orthoSize")
        .and_then(|v| v.as_f64())
        .unwrap_or(500.0) as f32;
    let near = config.get("near").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let far = config.get("far").and_then(|v| v.as_f64()).unwrap_or(2000.0) as f32;

    // Cache mesh and lights
    if let Some(Message::Bytes(b)) = payload.get("meshes") {
        use base64::Engine;
        ctx.pool_upsert(
            "_shadow",
            "mesh_b64",
            json!(base64::engine::general_purpose::STANDARD.encode(&**b)),
        );
    }
    if let Some(Message::Bytes(b)) = payload.get("lights") {
        use base64::Engine;
        ctx.pool_upsert(
            "_shadow",
            "lights_b64",
            json!(base64::engine::general_purpose::STANDARD.encode(&**b)),
        );
    }
    if let Some(Message::Integer(n)) = payload.get("light_count") {
        ctx.pool_upsert("_shadow", "light_count", json!(n));
    }

    let cache: HashMap<String, Value> = ctx.get_pool("_shadow").into_iter().collect();

    let mesh_data = match cache.get("mesh_b64").and_then(|v| v.as_str()) {
        Some(s) => {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(s)
                .unwrap_or_default()
        }
        None => return Ok(HashMap::new()),
    };

    let light_data = cache
        .get("lights_b64")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.decode(s).ok()
        });
    let light_count = cache
        .get("light_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    // Find first shadow-casting directional light
    let mut light_dir = [0.577f32, 0.577, -0.577]; // default
    if let Some(ref ld) = light_data {
        for i in 0..light_count.min(16) {
            let off = i * 64;
            if off + 64 > ld.len() {
                break;
            }
            let light_type = f32::from_le_bytes(ld[off + 12..off + 16].try_into().unwrap());
            let cast_shadow = f32::from_le_bytes(ld[off + 56..off + 60].try_into().unwrap());
            if light_type < 0.5 && cast_shadow > 0.5 {
                // Directional + shadow-casting
                light_dir = [
                    f32::from_le_bytes(ld[off + 16..off + 20].try_into().unwrap()),
                    f32::from_le_bytes(ld[off + 20..off + 24].try_into().unwrap()),
                    f32::from_le_bytes(ld[off + 24..off + 28].try_into().unwrap()),
                ];
                break;
            }
        }
    }

    // Build light view-projection matrix (orthographic for directional)
    let light_vp = build_light_vp(light_dir, ortho_size, near, far);

    // Render depth pass on CPU (simplified — proper GPU depth pass would use wgpu)
    let shadow_map = render_depth_cpu(&mesh_data, &light_vp, resolution, bias);

    // Output shadow matrix as bytes
    let mut mat_bytes = Vec::with_capacity(64);
    for row in &light_vp {
        for &v in row {
            mat_bytes.extend_from_slice(&v.to_le_bytes());
        }
    }

    let mut out = HashMap::new();
    out.insert("shadow_map".to_string(), Message::bytes(shadow_map));
    out.insert("shadow_matrix".to_string(), Message::bytes(mat_bytes));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "resolution": resolution,
            "bias": bias,
        }))),
    );
    Ok(out)
}

/// Build orthographic view-projection from directional light direction
fn build_light_vp(dir: [f32; 3], half_size: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2])
        .sqrt()
        .max(0.001);
    let fwd = [-dir[0] / len, -dir[1] / len, -dir[2] / len]; // light looks TOWARD scene

    // Position light far away along its direction
    let eye = [
        -fwd[0] * far * 0.5,
        -fwd[1] * far * 0.5,
        -fwd[2] * far * 0.5,
    ];

    let up = if fwd[1].abs() > 0.99 {
        [0.0f32, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let right = normalize_3(cross_3(up, fwd));
    let up = cross_3(fwd, right);

    // View matrix
    let view = [
        [right[0], up[0], -fwd[0], 0.0],
        [right[1], up[1], -fwd[1], 0.0],
        [right[2], up[2], -fwd[2], 0.0],
        [-dot_3(right, eye), -dot_3(up, eye), dot_3(fwd, eye), 1.0],
    ];

    // Orthographic projection
    let proj = [
        [1.0 / half_size, 0.0, 0.0, 0.0],
        [0.0, 1.0 / half_size, 0.0, 0.0],
        [0.0, 0.0, 1.0 / (far - near), 0.0],
        [0.0, 0.0, -near / (far - near), 1.0],
    ];

    mat4_mul_4(proj, view)
}

/// CPU depth rasterizer (simplified — triangle-level depth test)
fn render_depth_cpu(mesh_data: &[u8], light_vp: &[[f32; 4]; 4], res: u32, _bias: f32) -> Vec<u8> {
    let stride = 32; // assuming 32-byte stride (pos3+normal3+uv2)
    let stride_check = if mesh_data.len() % 32 == 0 { 32 } else { 24 };
    let actual_stride = stride_check;
    let vertex_count = mesh_data.len() / actual_stride;
    let tri_count = vertex_count / 3;

    let mut depth_buf = vec![1.0f32; (res * res) as usize];

    for tri in 0..tri_count {
        let mut clip = [[0.0f32; 4]; 3];
        for v in 0..3 {
            let off = (tri * 3 + v) * actual_stride;
            let px = f32::from_le_bytes(mesh_data[off..off + 4].try_into().unwrap());
            let py = f32::from_le_bytes(mesh_data[off + 4..off + 8].try_into().unwrap());
            let pz = f32::from_le_bytes(mesh_data[off + 8..off + 12].try_into().unwrap());
            // Transform by light VP
            clip[v] = transform_4(light_vp, [px, py, pz]);
        }

        // NDC
        let mut ndc = [[0.0f32; 3]; 3];
        let mut valid = true;
        for v in 0..3 {
            let w = clip[v][3];
            if w.abs() < 0.001 {
                valid = false;
                break;
            }
            ndc[v] = [clip[v][0] / w, clip[v][1] / w, clip[v][2] / w];
        }
        if !valid {
            continue;
        }

        // Bounding box in pixel space
        let r = res as f32;
        let mut min_x = r;
        let mut max_x = 0.0f32;
        let mut min_y = r;
        let mut max_y = 0.0f32;
        for v in 0..3 {
            let sx = (ndc[v][0] * 0.5 + 0.5) * r;
            let sy = (ndc[v][1] * 0.5 + 0.5) * r;
            min_x = min_x.min(sx);
            max_x = max_x.max(sx);
            min_y = min_y.min(sy);
            max_y = max_y.max(sy);
        }

        let x0 = (min_x as i32).max(0) as u32;
        let x1 = ((max_x as i32) + 1).min(res as i32) as u32;
        let y0 = (min_y as i32).max(0) as u32;
        let y1 = ((max_y as i32) + 1).min(res as i32) as u32;

        // Simple scanline: for each pixel in bbox, check if inside triangle and update depth
        let screen =
            |v: usize| -> [f32; 2] { [(ndc[v][0] * 0.5 + 0.5) * r, (ndc[v][1] * 0.5 + 0.5) * r] };
        let s0 = screen(0);
        let s1 = screen(1);
        let s2 = screen(2);

        for py in y0..y1 {
            for px in x0..x1 {
                let p = [px as f32 + 0.5, py as f32 + 0.5];
                let (w0, w1, w2) = barycentric(s0, s1, s2, p);
                if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                    let depth = w0 * ndc[0][2] + w1 * ndc[1][2] + w2 * ndc[2][2];
                    let idx = (py * res + px) as usize;
                    if depth < depth_buf[idx] {
                        depth_buf[idx] = depth;
                    }
                }
            }
        }
    }

    // Convert to bytes
    let mut bytes = Vec::with_capacity(depth_buf.len() * 4);
    for d in &depth_buf {
        bytes.extend_from_slice(&d.to_le_bytes());
    }
    bytes
}

fn barycentric(a: [f32; 2], b: [f32; 2], c: [f32; 2], p: [f32; 2]) -> (f32, f32, f32) {
    let v0 = [c[0] - a[0], c[1] - a[1]];
    let v1 = [b[0] - a[0], b[1] - a[1]];
    let v2 = [p[0] - a[0], p[1] - a[1]];
    let d00 = v0[0] * v0[0] + v0[1] * v0[1];
    let d01 = v0[0] * v1[0] + v0[1] * v1[1];
    let d02 = v0[0] * v2[0] + v0[1] * v2[1];
    let d11 = v1[0] * v1[0] + v1[1] * v1[1];
    let d12 = v1[0] * v2[0] + v1[1] * v2[1];
    let inv = 1.0 / (d00 * d11 - d01 * d01 + 0.00001);
    let u = (d11 * d02 - d01 * d12) * inv;
    let v = (d00 * d12 - d01 * d02) * inv;
    (1.0 - u - v, v, u)
}

fn transform_4(m: &[[f32; 4]; 4], p: [f32; 3]) -> [f32; 4] {
    [
        m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2] + m[3][0],
        m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2] + m[3][1],
        m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2] + m[3][2],
        m[0][3] * p[0] + m[1][3] * p[1] + m[2][3] * p[2] + m[3][3],
    ]
}

fn cross_3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot_3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize_3(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(0.001);
    [v[0] / l, v[1] / l, v[2] / l]
}
fn mat4_mul_4(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut r = [[0.0f32; 4]; 4];
    for c in 0..4 {
        for row in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k][row] * b[c][k];
            }
            r[c][row] = s;
        }
    }
    r
}
