//! Procedural tube mesh generator — creates a mesh along a Bezier curve.
//!
//! Bypasses SDF + MarchingCubes entirely. Directly generates triangle mesh
//! from cross-section circles along a curve. Zero artifacts by construction.
//!
//! Config:
//!   path: SVG-like path string (M, L, C, Q commands)
//!   profile: radius at evenly spaced points (interpolated)
//!   segments: number of cross-sections along the curve
//!   rings: number of vertices per cross-section circle (default 16)
//!   plane: "xz" | "xy" | "yz"

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

// Reuse the path parsing from SdfPathActor
use crate::gpu::sdf::path::{parse_path, sample_path, interpolate_profile, PathCmd};

#[actor(
    TubeMeshActor,
    inports::<1>(),
    outports::<1>(mesh, metadata),
    state(MemoryState)
)]
pub async fn tube_mesh_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let path_str = config.get("path").and_then(|v| v.as_str()).unwrap_or("M 0,0 L 5,0");
    let profile: Vec<f32> = config.get("profile").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
        .unwrap_or_else(|| vec![0.1]);
    let segments = config.get("segments").and_then(|v| v.as_u64()).unwrap_or(32) as usize;
    let rings = config.get("rings").and_then(|v| v.as_u64()).unwrap_or(16) as usize;
    let plane = config.get("plane").and_then(|v| v.as_str()).unwrap_or("xz");

    // Parse and sample path
    let commands = parse_path(path_str);
    let points_2d = sample_path(&commands, segments);
    if points_2d.len() < 2 {
        return Err(anyhow::anyhow!("Path needs at least 2 points"));
    }

    let radii = interpolate_profile(&profile, points_2d.len());

    // Convert 2D points to 3D
    let points: Vec<[f32; 3]> = points_2d.iter().map(|(x, y)| {
        match plane {
            "xy" => [*x, *y, 0.0],
            "yz" => [0.0, *x, *y],
            _ => [*x, 0.0, *y], // xz
        }
    }).collect();

    // Generate tube mesh
    let n = points.len();
    let vertex_count = n * rings;
    let triangle_count = (n - 1) * rings * 2;
    let stride = 24; // pos3 + normal3

    let mut mesh_data = Vec::with_capacity(triangle_count * 3 * stride);

    // Compute per-point tangent, normal, binormal (Frenet frame)
    let mut tangents: Vec<[f32; 3]> = Vec::with_capacity(n);
    for i in 0..n {
        let t = if i == 0 {
            sub3(points[1], points[0])
        } else if i == n - 1 {
            sub3(points[n - 1], points[n - 2])
        } else {
            sub3(points[i + 1], points[i - 1])
        };
        tangents.push(normalize3(t));
    }

    // Build cross-section circles using rotation-minimizing frame
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(n);
    let mut binormals: Vec<[f32; 3]> = Vec::with_capacity(n);

    // Initial frame: find a vector not parallel to the first tangent
    let t0 = tangents[0];
    let up = if t0[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let b0 = normalize3(cross3(t0, up));
    let n0 = cross3(b0, t0);
    normals.push(n0);
    binormals.push(b0);

    // Propagate frame along curve (rotation minimizing)
    for i in 1..n {
        let t_prev = tangents[i - 1];
        let t_curr = tangents[i];

        // Reflect previous frame across the tangent bisector
        let v1 = sub3(points[i], points[i - 1]);
        let c1 = dot3(v1, v1);
        if c1 < 1e-10 {
            normals.push(normals[i - 1]);
            binormals.push(binormals[i - 1]);
            continue;
        }

        let n_prev = normals[i - 1];
        let b_prev = binormals[i - 1];

        // Double reflection method for rotation minimizing frames
        let r_l = sub3(n_prev, scale3(v1, 2.0 * dot3(v1, n_prev) / c1));
        let r_b = sub3(b_prev, scale3(v1, 2.0 * dot3(v1, b_prev) / c1));

        let v2 = sub3(t_curr, sub3(t_prev, scale3(v1, 2.0 * dot3(v1, t_prev) / c1)));
        let c2 = dot3(v2, v2);
        if c2 < 1e-10 {
            normals.push(normalize3(r_l));
            binormals.push(normalize3(r_b));
            continue;
        }

        let new_n = sub3(r_l, scale3(v2, 2.0 * dot3(v2, r_l) / c2));
        let new_b = sub3(r_b, scale3(v2, 2.0 * dot3(v2, r_b) / c2));
        normals.push(normalize3(new_n));
        binormals.push(normalize3(new_b));
    }

    // Generate circle vertices at each cross-section
    let mut circle_verts: Vec<Vec<([f32; 3], [f32; 3])>> = Vec::with_capacity(n);
    for i in 0..n {
        let center = points[i];
        let r = radii[i];
        let nor = normals[i];
        let bin = binormals[i];

        let mut ring_verts = Vec::with_capacity(rings);
        for j in 0..rings {
            let angle = 2.0 * std::f32::consts::PI * j as f32 / rings as f32;
            let cos_a = angle.cos();
            let sin_a = angle.sin();

            // Position on circle
            let offset = add3(scale3(nor, cos_a * r), scale3(bin, sin_a * r));
            let pos = add3(center, offset);

            // Normal = outward direction from center
            let normal = normalize3(offset);

            ring_verts.push((pos, normal));
        }
        circle_verts.push(ring_verts);
    }

    // Generate triangles connecting adjacent rings
    for i in 0..n - 1 {
        let ring_a = &circle_verts[i];
        let ring_b = &circle_verts[i + 1];

        for j in 0..rings {
            let j_next = (j + 1) % rings;

            // Two triangles per quad
            let (pa, na) = ring_a[j];
            let (pb, nb) = ring_b[j];
            let (pc, nc) = ring_b[j_next];
            let (pd, nd) = ring_a[j_next];

            // Triangle 1: a, b, c
            emit_vertex(&mut mesh_data, pa, na);
            emit_vertex(&mut mesh_data, pb, nb);
            emit_vertex(&mut mesh_data, pc, nc);

            // Triangle 2: a, c, d
            emit_vertex(&mut mesh_data, pa, na);
            emit_vertex(&mut mesh_data, pc, nc);
            emit_vertex(&mut mesh_data, pd, nd);
        }
    }

    // Cap the ends
    // Start cap
    {
        let center = points[0];
        let nor = scale3(tangents[0], -1.0); // inward-facing normal
        for j in 0..rings {
            let j_next = (j + 1) % rings;
            emit_vertex(&mut mesh_data, center, nor);
            emit_vertex(&mut mesh_data, circle_verts[0][j_next].0, nor);
            emit_vertex(&mut mesh_data, circle_verts[0][j].0, nor);
        }
    }
    // End cap
    {
        let center = points[n - 1];
        let nor = tangents[n - 1];
        for j in 0..rings {
            let j_next = (j + 1) % rings;
            emit_vertex(&mut mesh_data, center, nor);
            emit_vertex(&mut mesh_data, circle_verts[n - 1][j].0, nor);
            emit_vertex(&mut mesh_data, circle_verts[n - 1][j_next].0, nor);
        }
    }

    let total_verts = mesh_data.len() / stride;

    let mut out = HashMap::new();
    out.insert("mesh".to_string(), Message::bytes(mesh_data));
    out.insert("metadata".to_string(), Message::object(EncodableValue::from(json!({
        "vertexCount": total_verts,
        "triangleCount": total_verts / 3,
        "segments": n,
        "rings": rings,
        "stride": stride,
        "format": "pos3_normal3_f32",
    }))));
    Ok(out)
}

fn emit_vertex(buf: &mut Vec<u8>, pos: [f32; 3], nor: [f32; 3]) {
    for f in &pos { buf.extend_from_slice(&f.to_le_bytes()); }
    for f in &nor { buf.extend_from_slice(&f.to_le_bytes()); }
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale3(v: [f32; 3], s: f32) -> [f32; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}
fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1]*b[2] - a[2]*b[1], a[2]*b[0] - a[0]*b[2], a[0]*b[1] - a[1]*b[0]]
}
fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0]*v[0] + v[1]*v[1] + v[2]*v[2]).sqrt();
    if l > 1e-8 { [v[0]/l, v[1]/l, v[2]/l] } else { [0.0, 1.0, 0.0] }
}
