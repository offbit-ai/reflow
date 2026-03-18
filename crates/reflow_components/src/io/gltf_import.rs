//! glTF/GLB import actor.
//!
//! Parses glTF 2.0 (JSON + binary) or GLB (single binary) files and
//! extracts mesh, skeleton, animation clip, and skin weight data in
//! Reflow's internal formats.
//!
//! ## Outports
//!
//! - `mesh` — 24-byte stride (pos3+normal3) or 36-byte (pos3+normal3+color3)
//! - `skeleton` — JSON skeleton descriptor (same format as SkeletonActor output)
//! - `inverse_bind_matrices` — flat f32 LE bytes, 64 bytes per bone
//! - `clip` — JSON animation clip (same format as AnimationClipActor output)
//! - `skin` — skin weight bytes (per vertex: maxInfluences × (u16 + f32))
//! - `metadata` — JSON summary of the imported scene

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    GltfImportActor,
    inports::<10>(file_data),
    outports::<1>(mesh, skeleton, inverse_bind_matrices, clip, skin, skin_descriptor, metadata, error),
    state(MemoryState)
)]
pub async fn gltf_import_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let data = match payload.get("file_data") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on file_data port")),
    };

    import_gltf(&data, &config)
}

pub(crate) fn import_gltf(
    data: &[u8],
    config: &HashMap<String, Value>,
) -> Result<HashMap<String, Message>, Error> {
    let mesh_index = config
        .get("meshIndex")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let skin_index = config
        .get("skinIndex")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let anim_index = config
        .get("animationIndex")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let max_influences = config
        .get("maxInfluences")
        .and_then(|v| v.as_u64())
        .unwrap_or(4) as usize;
    let name = config
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("imported");

    // Parse glTF
    let gltf =
        gltf::Gltf::from_slice(data).map_err(|e| anyhow::anyhow!("Failed to parse glTF: {}", e))?;

    // Resolve binary buffers
    let buffers = load_buffers(&gltf, data)?;

    let mut out = HashMap::new();
    let mut meta = json!({ "name": name, "format": "gltf" });

    // ─── Mesh ───
    if let Some(mesh) = gltf.meshes().nth(mesh_index) {
        let (mesh_bytes, vertex_count, has_colors) = extract_mesh(&mesh, &buffers)?;
        let stride = if has_colors { 36 } else { 24 };
        meta["meshName"] = json!(mesh.name().unwrap_or("mesh"));
        meta["vertexCount"] = json!(vertex_count);
        meta["triangleCount"] = json!(vertex_count / 3);
        meta["stride"] = json!(stride);
        meta["hasColors"] = json!(has_colors);
        out.insert("mesh".to_string(), Message::bytes(mesh_bytes));
    }
    meta["meshCount"] = json!(gltf.meshes().count());

    // ─── Skin (skeleton + weights) ───
    if let Some(skin) = gltf.skins().nth(skin_index) {
        let joints: Vec<_> = skin.joints().collect();
        let bone_count = joints.len();

        // Build skeleton JSON
        let skeleton = extract_skeleton(&skin, &gltf)?;
        meta["boneCount"] = json!(bone_count);
        meta["skeletonName"] = json!(skin.name().unwrap_or("skeleton"));
        out.insert(
            "skeleton".to_string(),
            Message::object(EncodableValue::from(skeleton)),
        );

        // Inverse bind matrices
        if let Some(accessor) = skin.inverse_bind_matrices() {
            let ibm_bytes = read_accessor_raw(&accessor, &buffers);
            out.insert(
                "inverse_bind_matrices".to_string(),
                Message::bytes(ibm_bytes),
            );
        }

        // Skin weights from the mesh that references this skin
        if let Some(mesh_node) = gltf
            .nodes()
            .find(|n| n.skin().map(|s| s.index()) == Some(skin.index()))
        {
            if let Some(mesh) = mesh_node.mesh() {
                let (weights_bytes, skin_desc) =
                    extract_skin_weights(&mesh, &joints, max_influences, &buffers)?;
                out.insert("skin".to_string(), Message::bytes(weights_bytes));
                out.insert(
                    "skin_descriptor".to_string(),
                    Message::object(EncodableValue::from(skin_desc)),
                );
            }
        }
    }
    meta["skinCount"] = json!(gltf.skins().count());

    // ─── Animation ───
    let anim_names: Vec<_> = gltf
        .animations()
        .map(|a| a.name().unwrap_or("anim").to_string())
        .collect();
    meta["animationCount"] = json!(anim_names.len());
    meta["animationNames"] = json!(anim_names);

    if let Some(anim) = gltf.animations().nth(anim_index) {
        // Need joint-to-bone-index mapping from the skin
        let joint_map: HashMap<usize, usize> = if let Some(skin) = gltf.skins().nth(skin_index) {
            skin.joints()
                .enumerate()
                .map(|(bone_idx, joint)| (joint.index(), bone_idx))
                .collect()
        } else {
            HashMap::new()
        };

        let clip = extract_animation(&anim, &joint_map, &buffers)?;
        out.insert(
            "clip".to_string(),
            Message::object(EncodableValue::from(clip)),
        );
    }

    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(meta)),
    );
    Ok(out)
}

// ═══════════════════════════════════════════════════════════════════════════
// Buffer resolution
// ═══════════════════════════════════════════════════════════════════════════

fn load_buffers(gltf: &gltf::Gltf, data: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
    let mut buffers = Vec::new();
    for buffer in gltf.buffers() {
        match buffer.source() {
            gltf::buffer::Source::Bin => {
                // GLB embedded binary chunk
                if let Some(blob) = gltf.blob.as_ref() {
                    buffers.push(blob.clone());
                } else {
                    buffers.push(Vec::new());
                }
            }
            gltf::buffer::Source::Uri(uri) => {
                if let Some(base64_data) = uri.strip_prefix("data:") {
                    // Data URI
                    if let Some(comma) = base64_data.find(',') {
                        let encoded = &base64_data[comma + 1..];
                        use base64::Engine;
                        let decoded = base64::engine::general_purpose::STANDARD
                            .decode(encoded)
                            .unwrap_or_default();
                        buffers.push(decoded);
                    } else {
                        buffers.push(Vec::new());
                    }
                } else {
                    // External URI — not supported in streaming context
                    // Try to read from the data itself (some exporters embed everything)
                    let _ = data;
                    buffers.push(Vec::new());
                }
            }
        }
    }
    Ok(buffers)
}

// ═══════════════════════════════════════════════════════════════════════════
// Accessor reading
// ═══════════════════════════════════════════════════════════════════════════

fn read_accessor_raw(accessor: &gltf::Accessor, buffers: &[Vec<u8>]) -> Vec<u8> {
    let view = match accessor.view() {
        Some(v) => v,
        None => return Vec::new(),
    };
    let buf_idx = view.buffer().index();
    if buf_idx >= buffers.len() {
        return Vec::new();
    }

    let stride = view.stride().unwrap_or(accessor.size());
    let offset = view.offset() + accessor.offset();
    let count = accessor.count();
    let elem_size = accessor.size();
    let buf = &buffers[buf_idx];

    let mut result = Vec::with_capacity(count * elem_size);
    for i in 0..count {
        let start = offset + i * stride;
        let end = start + elem_size;
        if end <= buf.len() {
            result.extend_from_slice(&buf[start..end]);
        }
    }
    result
}

fn read_accessor_f32(accessor: &gltf::Accessor, buffers: &[Vec<u8>]) -> Vec<f32> {
    let raw = read_accessor_raw(accessor, buffers);
    raw.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn read_accessor_u16(accessor: &gltf::Accessor, buffers: &[Vec<u8>]) -> Vec<u16> {
    let raw = read_accessor_raw(accessor, buffers);
    match accessor.data_type() {
        gltf::accessor::DataType::U8 => raw.iter().map(|&b| b as u16).collect(),
        gltf::accessor::DataType::U16 => raw
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect(),
        gltf::accessor::DataType::U32 => raw
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u16)
            .collect(),
        _ => Vec::new(),
    }
}

fn read_indices(accessor: &gltf::Accessor, buffers: &[Vec<u8>]) -> Vec<u32> {
    let raw = read_accessor_raw(accessor, buffers);
    match accessor.data_type() {
        gltf::accessor::DataType::U8 => raw.iter().map(|&b| b as u32).collect(),
        gltf::accessor::DataType::U16 => raw
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as u32)
            .collect(),
        gltf::accessor::DataType::U32 => raw
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
        _ => Vec::new(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Mesh extraction
// ═══════════════════════════════════════════════════════════════════════════

fn extract_mesh(mesh: &gltf::Mesh, buffers: &[Vec<u8>]) -> Result<(Vec<u8>, usize, bool), Error> {
    let mut all_bytes: Vec<u8> = Vec::new();
    let mut total_verts = 0usize;
    let mut has_colors = false;

    for primitive in mesh.primitives() {
        let positions = primitive
            .get(&gltf::Semantic::Positions)
            .ok_or_else(|| anyhow::anyhow!("Primitive missing POSITION attribute"))?;
        let pos_data = read_accessor_f32(&positions, buffers);

        let normals_data = primitive
            .get(&gltf::Semantic::Normals)
            .map(|a| read_accessor_f32(&a, buffers));

        let colors_data = primitive
            .get(&gltf::Semantic::Colors(0))
            .map(|a| read_accessor_f32(&a, buffers));

        let prim_has_colors = colors_data.is_some();
        if prim_has_colors {
            has_colors = true;
        }

        let vert_count = pos_data.len() / 3;

        // Build de-indexed vertices
        let indices: Option<Vec<u32>> = primitive.indices().map(|acc| read_indices(&acc, buffers));

        let emit_vertex = |vi: usize, out: &mut Vec<u8>| {
            // Position (12 bytes)
            for j in 0..3 {
                let val = pos_data.get(vi * 3 + j).copied().unwrap_or(0.0);
                out.extend_from_slice(&val.to_le_bytes());
            }
            // Normal (12 bytes)
            if let Some(ref normals) = normals_data {
                for j in 0..3 {
                    let val = normals.get(vi * 3 + j).copied().unwrap_or(0.0);
                    out.extend_from_slice(&val.to_le_bytes());
                }
            } else {
                out.extend_from_slice(&[0u8; 12]);
            }
            // Color (12 bytes) — only if any primitive has colors
            if has_colors {
                if let Some(ref colors) = colors_data {
                    // glTF colors can be VEC3 or VEC4
                    let components = colors.len() / vert_count;
                    for j in 0..3 {
                        let val = if j < components {
                            colors.get(vi * components + j).copied().unwrap_or(1.0)
                        } else {
                            1.0
                        };
                        out.extend_from_slice(&val.to_le_bytes());
                    }
                } else {
                    // Default white
                    out.extend_from_slice(&1.0f32.to_le_bytes());
                    out.extend_from_slice(&1.0f32.to_le_bytes());
                    out.extend_from_slice(&1.0f32.to_le_bytes());
                }
            }
        };

        if let Some(idx) = indices {
            for &i in &idx {
                emit_vertex(i as usize, &mut all_bytes);
            }
            total_verts += idx.len();
        } else {
            for i in 0..vert_count {
                emit_vertex(i, &mut all_bytes);
            }
            total_verts += vert_count;
        }
    }

    // Compute normals for any zero-length normals
    let stride = if has_colors { 36 } else { 24 };
    compute_face_normals_if_missing(&mut all_bytes, stride);

    Ok((all_bytes, total_verts, has_colors))
}

fn compute_face_normals_if_missing(mesh: &mut [u8], stride: usize) {
    let vertex_count = mesh.len() / stride;
    let tri_count = vertex_count / 3;

    fn rf(mesh: &[u8], off: usize) -> f32 {
        f32::from_le_bytes([mesh[off], mesh[off + 1], mesh[off + 2], mesh[off + 3]])
    }

    for t in 0..tri_count {
        let b0 = t * 3 * stride;
        let b1 = b0 + stride;
        let b2 = b0 + 2 * stride;

        let n_sq =
            rf(mesh, b0 + 12).powi(2) + rf(mesh, b0 + 16).powi(2) + rf(mesh, b0 + 20).powi(2);

        if n_sq < 1e-10 {
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

// ═══════════════════════════════════════════════════════════════════════════
// Skeleton extraction
// ═══════════════════════════════════════════════════════════════════════════

fn extract_skeleton(skin: &gltf::Skin, gltf: &gltf::Gltf) -> Result<Value, Error> {
    let joints: Vec<_> = skin.joints().collect();
    let bone_count = joints.len();

    // Map node index -> bone index
    let node_to_bone: HashMap<usize, usize> = joints
        .iter()
        .enumerate()
        .map(|(bi, j)| (j.index(), bi))
        .collect();

    // Build parent map from node hierarchy
    let mut node_parent: HashMap<usize, usize> = HashMap::new();
    for node in gltf.nodes() {
        for child in node.children() {
            node_parent.insert(child.index(), node.index());
        }
    }

    let mut bones = Vec::with_capacity(bone_count);
    for (bi, joint) in joints.iter().enumerate() {
        let (t, r, s) = joint.transform().decomposed();

        // Parent: find parent node, map to bone index
        let parent_idx = node_parent
            .get(&joint.index())
            .and_then(|&parent_node| node_to_bone.get(&parent_node))
            .map(|&idx| idx as i64)
            .unwrap_or(-1);

        // Build local bind transform matrix
        let local_mat = trs_to_mat4_f32(t, r, s);

        bones.push(json!({
            "index": bi,
            "name": joint.name().unwrap_or(&format!("bone_{}", bi)),
            "parent": parent_idx,
            "localBindTransform": local_mat.to_vec(),
        }));
    }

    Ok(json!({
        "name": skin.name().unwrap_or("skeleton"),
        "boneCount": bone_count,
        "bones": bones,
    }))
}

fn trs_to_mat4_f32(t: [f32; 3], r: [f32; 4], s: [f32; 3]) -> [f32; 16] {
    // Quaternion [x, y, z, w] to rotation matrix
    let (x, y, z, w) = (r[0] as f64, r[1] as f64, r[2] as f64, r[3] as f64);
    let x2 = x + x;
    let y2 = y + y;
    let z2 = z + z;
    let xx = x * x2;
    let xy = x * y2;
    let xz = x * z2;
    let yy = y * y2;
    let yz = y * z2;
    let zz = z * z2;
    let wx = w * x2;
    let wy = w * y2;
    let wz = w * z2;

    let sx = s[0] as f64;
    let sy = s[1] as f64;
    let sz = s[2] as f64;

    [
        ((1.0 - (yy + zz)) * sx) as f32,
        ((xy + wz) * sx) as f32,
        ((xz - wy) * sx) as f32,
        0.0,
        ((xy - wz) * sy) as f32,
        ((1.0 - (xx + zz)) * sy) as f32,
        ((yz + wx) * sy) as f32,
        0.0,
        ((xz + wy) * sz) as f32,
        ((yz - wx) * sz) as f32,
        ((1.0 - (xx + yy)) * sz) as f32,
        0.0,
        t[0],
        t[1],
        t[2],
        1.0,
    ]
}

// ═══════════════════════════════════════════════════════════════════════════
// Skin weights extraction
// ═══════════════════════════════════════════════════════════════════════════

fn extract_skin_weights(
    mesh: &gltf::Mesh,
    joints: &[gltf::Node],
    max_influences: usize,
    buffers: &[Vec<u8>],
) -> Result<(Vec<u8>, Value), Error> {
    let mut all_joints: Vec<u16> = Vec::new();
    let mut all_weights: Vec<f32> = Vec::new();
    let mut total_verts = 0usize;

    for primitive in mesh.primitives() {
        let joints_acc = primitive.get(&gltf::Semantic::Joints(0));
        let weights_acc = primitive.get(&gltf::Semantic::Weights(0));

        let positions = primitive
            .get(&gltf::Semantic::Positions)
            .ok_or_else(|| anyhow::anyhow!("Missing POSITION"))?;

        let vert_count = positions.count();
        let indices: Option<Vec<u32>> = primitive.indices().map(|acc| read_indices(&acc, buffers));

        let (j_data, w_data) = match (joints_acc, weights_acc) {
            (Some(j), Some(w)) => (
                read_accessor_u16(&j, buffers),
                read_accessor_f32(&w, buffers),
            ),
            _ => {
                // No skin weights — fill with zeros
                let count = if let Some(ref idx) = indices {
                    idx.len()
                } else {
                    vert_count
                };
                all_joints.extend(std::iter::repeat(0u16).take(count * 4));
                all_weights.extend(std::iter::repeat(0.0f32).take(count * 4));
                total_verts += count;
                continue;
            }
        };

        // glTF provides 4 joints + 4 weights per vertex
        if let Some(ref idx) = indices {
            for &i in idx {
                let vi = i as usize;
                for k in 0..4 {
                    all_joints.push(*j_data.get(vi * 4 + k).unwrap_or(&0));
                    all_weights.push(*w_data.get(vi * 4 + k).unwrap_or(&0.0));
                }
            }
            total_verts += idx.len();
        } else {
            all_joints.extend_from_slice(&j_data);
            all_weights.extend_from_slice(&w_data);
            total_verts += vert_count;
        }
    }

    // Pack into Reflow format: per vertex, maxInfluences * (u16 bone_index + f32 weight)
    let entry_size = 2 + 4; // u16 + f32
    let mut out = Vec::with_capacity(total_verts * max_influences * entry_size);

    for v in 0..total_verts {
        // Collect this vertex's influences (glTF gives 4)
        let mut influences: Vec<(u16, f32)> = Vec::with_capacity(4);
        for k in 0..4 {
            let idx = v * 4 + k;
            let j = all_joints.get(idx).copied().unwrap_or(0);
            let w = all_weights.get(idx).copied().unwrap_or(0.0);
            if w > 0.0 {
                influences.push((j, w));
            }
        }

        // Sort by weight descending, take top maxInfluences
        influences.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        influences.truncate(max_influences);

        // Normalize weights
        let total: f32 = influences.iter().map(|(_, w)| w).sum();
        if total > 0.0 {
            for inf in &mut influences {
                inf.1 /= total;
            }
        }

        // Write entries, pad with zeros if fewer than maxInfluences
        for k in 0..max_influences {
            let (j, w) = influences.get(k).copied().unwrap_or((0, 0.0));
            out.extend_from_slice(&j.to_le_bytes());
            out.extend_from_slice(&w.to_le_bytes());
        }
    }

    let descriptor = json!({
        "vertexCount": total_verts,
        "maxInfluences": max_influences,
        "boneCount": joints.len(),
    });

    Ok((out, descriptor))
}

// ═══════════════════════════════════════════════════════════════════════════
// Animation extraction
// ═══════════════════════════════════════════════════════════════════════════

fn extract_animation(
    anim: &gltf::Animation,
    joint_map: &HashMap<usize, usize>,
    buffers: &[Vec<u8>],
) -> Result<Value, Error> {
    let mut channels = Vec::new();
    let mut max_time: f64 = 0.0;

    for channel in anim.channels() {
        let target_node = channel.target().node().index();

        // Map node index to bone index
        let bone_index = match joint_map.get(&target_node) {
            Some(&bi) => bi,
            None => continue, // Not a skeleton bone, skip
        };

        let property = match channel.target().property() {
            gltf::animation::Property::Translation => "position",
            gltf::animation::Property::Rotation => "rotation",
            gltf::animation::Property::Scale => "scale",
            gltf::animation::Property::MorphTargetWeights => continue, // Skip morph targets
        };

        let sampler = channel.sampler();
        let interpolation = match sampler.interpolation() {
            gltf::animation::Interpolation::Linear => "linear",
            gltf::animation::Interpolation::Step => "step",
            gltf::animation::Interpolation::CubicSpline => "linear", // Flatten for now
        };

        let input = sampler.input();
        let output = sampler.output();

        let times = read_accessor_f32(&input, buffers);
        let values_flat = read_accessor_f32(&output, buffers);

        if let Some(&last_t) = times.last() {
            max_time = max_time.max(last_t as f64);
        }

        // Group values by keyframe
        let components = match property {
            "rotation" => 4, // quaternion [x,y,z,w]
            _ => 3,          // vec3
        };

        let is_cubic = matches!(
            sampler.interpolation(),
            gltf::animation::Interpolation::CubicSpline
        );

        let values: Vec<Value> = if is_cubic {
            // CubicSpline: 3 * components per keyframe (in-tangent, value, out-tangent)
            // Extract just the value (middle third)
            times
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let base = i * 3 * components + components; // skip in-tangent
                    let v: Vec<f64> = (0..components)
                        .map(|j| values_flat.get(base + j).copied().unwrap_or(0.0) as f64)
                        .collect();
                    json!(v)
                })
                .collect()
        } else {
            times
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let base = i * components;
                    let v: Vec<f64> = (0..components)
                        .map(|j| values_flat.get(base + j).copied().unwrap_or(0.0) as f64)
                        .collect();
                    json!(v)
                })
                .collect()
        };

        let times_f64: Vec<f64> = times.iter().map(|&t| t as f64).collect();

        channels.push(json!({
            "boneIndex": bone_index,
            "property": property,
            "interpolation": interpolation,
            "times": times_f64,
            "values": values,
        }));
    }

    Ok(json!({
        "name": anim.name().unwrap_or("animation"),
        "duration": max_time,
        "channelCount": channels.len(),
        "channels": channels,
    }))
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
