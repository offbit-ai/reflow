//! OBJ mesh import actor.
//!
//! Parses Wavefront OBJ data and outputs mesh bytes in Reflow's
//! 24-byte stride (pos3+normal3 f32 per vertex) format.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    ObjImportActor,
    inports::<10>(file_data),
    outports::<1>(mesh, metadata, error),
    state(MemoryState)
)]
pub async fn obj_import_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();

    let data = match payload.get("file_data") {
        Some(Message::Bytes(b)) => b.clone(),
        Some(Message::String(s)) => std::sync::Arc::new(s.as_bytes().to_vec()),
        _ => return Ok(error_output("Expected Bytes or String on file_data port")),
    };

    parse_obj(&data)
}

pub(crate) fn parse_obj(data: &[u8]) -> Result<HashMap<String, Message>, Error> {
    let text =
        std::str::from_utf8(data).map_err(|e| anyhow::anyhow!("OBJ is not valid UTF-8: {}", e))?;

    let mut cursor = std::io::Cursor::new(text.as_bytes());
    let load_options = tobj::LoadOptions {
        triangulate: true,
        single_index: true,
        ..Default::default()
    };

    let (models, _materials) = tobj::load_obj_buf(&mut cursor, &load_options, |_| {
        Err(tobj::LoadError::GenericFailure)
    })?;

    if models.is_empty() {
        return Ok(error_output("OBJ file contains no models"));
    }

    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;
    let mut mesh_bytes: Vec<u8> = Vec::new();

    for model in &models {
        let m = &model.mesh;
        let has_normals = !m.normals.is_empty();
        let vert_count = m.positions.len() / 3;

        if m.indices.is_empty() {
            // Non-indexed: positions are already in triangle order
            for i in 0..vert_count {
                // Position
                for j in 0..3 {
                    mesh_bytes.extend_from_slice(&m.positions[i * 3 + j].to_le_bytes());
                }
                // Normal
                if has_normals && i * 3 + 2 < m.normals.len() {
                    for j in 0..3 {
                        mesh_bytes.extend_from_slice(&m.normals[i * 3 + j].to_le_bytes());
                    }
                } else {
                    mesh_bytes.extend_from_slice(&[0; 12]); // placeholder
                }
            }
            total_vertices += vert_count;
            total_triangles += vert_count / 3;
        } else {
            // Indexed: de-index into flat triangle soup
            let idx_count = m.indices.len();
            for &idx in &m.indices {
                let i = idx as usize;
                // Position
                for j in 0..3 {
                    let val = if i * 3 + j < m.positions.len() {
                        m.positions[i * 3 + j]
                    } else {
                        0.0
                    };
                    mesh_bytes.extend_from_slice(&val.to_le_bytes());
                }
                // Normal
                if has_normals {
                    let ni = if !m.normal_indices.is_empty() {
                        m.normal_indices
                            .get(total_vertices + i)
                            .copied()
                            .unwrap_or(idx) as usize
                    } else {
                        i
                    };
                    for j in 0..3 {
                        let val = if ni * 3 + j < m.normals.len() {
                            m.normals[ni * 3 + j]
                        } else {
                            0.0
                        };
                        mesh_bytes.extend_from_slice(&val.to_le_bytes());
                    }
                } else {
                    mesh_bytes.extend_from_slice(&[0; 12]);
                }
            }
            total_vertices += idx_count;
            total_triangles += idx_count / 3;
        }
    }

    // Compute face normals for vertices that have zero normals
    compute_missing_normals(&mut mesh_bytes);

    let mut out = HashMap::new();
    out.insert("mesh".to_string(), Message::bytes(mesh_bytes));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "format": "obj",
            "modelCount": models.len(),
            "modelNames": models.iter().map(|m| m.name.clone()).collect::<Vec<_>>(),
            "triangleCount": total_triangles,
            "vertexCount": total_vertices,
            "stride": 24,
        }))),
    );
    Ok(out)
}

/// Fill in zero-length normals with computed face normals.
fn compute_missing_normals(mesh: &mut Vec<u8>) {
    let vertex_count = mesh.len() / 24;
    let tri_count = vertex_count / 3;

    fn rf(mesh: &[u8], off: usize) -> f32 {
        f32::from_le_bytes([mesh[off], mesh[off + 1], mesh[off + 2], mesh[off + 3]])
    }

    for t in 0..tri_count {
        let b0 = t * 3 * 24;
        let b1 = b0 + 24;
        let b2 = b0 + 48;

        let n0_len_sq =
            rf(mesh, b0 + 12).powi(2) + rf(mesh, b0 + 16).powi(2) + rf(mesh, b0 + 20).powi(2);

        if n0_len_sq < 1e-10 {
            let p0 = [rf(mesh, b0), rf(mesh, b0 + 4), rf(mesh, b0 + 8)];
            let p1 = [rf(mesh, b1), rf(mesh, b1 + 4), rf(mesh, b1 + 8)];
            let p2 = [rf(mesh, b2), rf(mesh, b2 + 4), rf(mesh, b2 + 8)];

            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let nn = if len > 1e-6 {
                [n[0] / len, n[1] / len, n[2] / len]
            } else {
                [0.0, 1.0, 0.0]
            };

            for base in [b0, b1, b2] {
                mesh[base + 12..base + 16].copy_from_slice(&nn[0].to_le_bytes());
                mesh[base + 16..base + 20].copy_from_slice(&nn[1].to_le_bytes());
                mesh[base + 20..base + 24].copy_from_slice(&nn[2].to_le_bytes());
            }
        }
    }
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
