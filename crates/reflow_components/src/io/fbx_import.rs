//! FBX import actor.
//!
//! Parses Autodesk FBX binary files (v7400+, including Mixamo v7700)
//! and extracts mesh, skeleton, animation clip, and skin weight data
//! in Reflow's internal formats — same outport schema as GltfImportActor.
//!
//! Uses `fbxcel` for low-level FBX binary parsing with custom extraction
//! of geometry, skeleton hierarchy, animation curves, and skin deformers.
//!
//! ## Outports
//!
//! - `mesh` — 24-byte stride (pos3+normal3) vertex bytes
//! - `skeleton` — JSON skeleton descriptor
//! - `inverse_bind_matrices` — flat f32 LE bytes, 64 bytes per bone
//! - `clip` — JSON animation clip
//! - `skin` — skin weight bytes (per vertex: maxInfluences × (u16 + f32))
//! - `skin_descriptor` — JSON { vertexCount, maxInfluences, boneCount }
//! - `metadata` — JSON summary
//! - `error` — error message if import fails

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use fbxcel::low::v7400::AttributeValue;
use fbxcel::pull_parser::v7400::attribute::loaders::DirectLoader;
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufReader, Cursor};

#[actor(
    FbxImportActor,
    inports::<10>(file_data),
    outports::<1>(mesh, skeleton, inverse_bind_matrices, clip, skin, skin_descriptor, metadata, error),
    state(MemoryState)
)]
pub async fn fbx_import_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let _config = ctx.get_config_hashmap();

    let data = match payload.get("file_data") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on file_data port")),
    };

    match import_fbx(&data) {
        Ok(out) => Ok(out),
        Err(e) => Ok(error_output(&format!("FBX import failed: {}", e))),
    }
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}

// ═══════════════════════════════════════════════════════════════
// FBX parsing
// ═══════════════════════════════════════════════════════════════

fn import_fbx(data: &[u8]) -> Result<HashMap<String, Message>> {
    use fbxcel::pull_parser::any::AnyParser;

    let reader = BufReader::new(Cursor::new(data));
    let parser = AnyParser::from_seekable_reader(reader)
        .map_err(|e| anyhow::anyhow!("FBX parse error: {:?}", e))?;

    let mut parser = match parser {
        AnyParser::V7400(p) => p,
        _ => return Err(anyhow::anyhow!("Unsupported FBX version (need v7400+)")),
    };

    // Collect all FBX nodes into a tree structure for easier traversal
    let tree = collect_fbx_tree(&mut parser)?;

    let mut out = HashMap::new();

    // Extract mesh geometry
    let (vertices, normals, indices) = extract_geometry(&tree)?;
    let (mesh_bytes, tri_to_control) = build_mesh_bytes(&vertices, &normals, &indices);
    out.insert("mesh".to_string(), Message::bytes(mesh_bytes));

    // Extract skeleton hierarchy (includes per-bone PreRotation for animation)
    let (skeleton, bone_names, pre_rotations) = extract_skeleton(&tree)?;
    out.insert(
        "skeleton".to_string(),
        Message::object(EncodableValue::from(skeleton.clone())),
    );

    // Extract animation curves (composes PreRotation with rotation keyframes)
    let clip = extract_animation(&tree, &bone_names, &pre_rotations)?;
    out.insert(
        "clip".to_string(),
        Message::object(EncodableValue::from(clip)),
    );

    // Extract skin weights from Deformer/Cluster nodes (per control-point vertex)
    let control_vert_count = vertices.len() / 3;
    let (cp_skin_bytes, skin_desc) =
        extract_skin_weights(&tree, control_vert_count, bone_names.len())?;

    // Expand skin weights to match triangulated mesh (per triangle-vertex)
    let max_influences = 4;
    let entry_size = 6; // u16 + f32
    let weights_per_vert = max_influences * entry_size;
    let tri_vert_count = tri_to_control.len();
    let mut skin_bytes = Vec::with_capacity(tri_vert_count * weights_per_vert);
    for &cp_idx in &tri_to_control {
        let src_off = cp_idx * weights_per_vert;
        if src_off + weights_per_vert <= cp_skin_bytes.len() {
            skin_bytes.extend_from_slice(&cp_skin_bytes[src_off..src_off + weights_per_vert]);
        } else {
            // Fallback: bone 0, weight 1.0
            for j in 0..max_influences {
                skin_bytes.extend_from_slice(&0u16.to_le_bytes());
                let w: f32 = if j == 0 { 1.0 } else { 0.0 };
                skin_bytes.extend_from_slice(&w.to_le_bytes());
            }
        }
    }

    // Update skin descriptor with triangle-vertex count
    let skin_desc = json!({
        "vertexCount": tri_vert_count,
        "maxInfluences": max_influences,
        "boneCount": bone_names.len(),
    });
    out.insert("skin".to_string(), Message::bytes(skin_bytes));
    out.insert(
        "skin_descriptor".to_string(),
        Message::object(EncodableValue::from(skin_desc)),
    );

    // Extract inverse bind matrices from Deformer clusters
    let ibm_bytes = extract_inverse_bind_matrices(&tree, bone_names.len())?;
    out.insert(
        "inverse_bind_matrices".to_string(),
        Message::bytes(ibm_bytes),
    );

    // Metadata
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "format": "fbx",
            "vertices": vertices.len() / 3,
            "bones": bone_names.len(),
            "boneNames": bone_names,
        }))),
    );

    Ok(out)
}

// ═══════════════════════════════════════════════════════════════
// FBX Tree — collect nodes into traversable structure
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Default)]
struct FbxNode {
    name: String,
    attributes: Vec<AttributeValue>,
    children: Vec<FbxNode>,
}

impl FbxNode {
    fn child(&self, name: &str) -> Option<&FbxNode> {
        self.children.iter().find(|c| c.name == name)
    }

    fn children_named(&self, name: &str) -> Vec<&FbxNode> {
        self.children.iter().filter(|c| c.name == name).collect()
    }

    fn attr_str(&self, idx: usize) -> Option<&str> {
        match self.attributes.get(idx) {
            Some(AttributeValue::String(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    fn attr_i64(&self, idx: usize) -> Option<i64> {
        match self.attributes.get(idx) {
            Some(AttributeValue::I64(v)) => Some(*v),
            Some(AttributeValue::I32(v)) => Some(*v as i64),
            _ => None,
        }
    }

    fn attr_f64_arr(&self, idx: usize) -> Option<&[f64]> {
        match self.attributes.get(idx) {
            Some(AttributeValue::ArrF64(v)) => Some(v),
            _ => None,
        }
    }

    fn attr_i32_arr(&self, idx: usize) -> Option<&[i32]> {
        match self.attributes.get(idx) {
            Some(AttributeValue::ArrI32(v)) => Some(v),
            _ => None,
        }
    }

    fn attr_i64_arr(&self, idx: usize) -> Option<&[i64]> {
        match self.attributes.get(idx) {
            Some(AttributeValue::ArrI64(v)) => Some(v),
            _ => None,
        }
    }
}

fn collect_fbx_tree<R: std::io::Read + std::io::Seek>(
    parser: &mut fbxcel::pull_parser::v7400::Parser<R>,
) -> Result<FbxNode> {
    use fbxcel::pull_parser::v7400::Event;

    let mut root = FbxNode {
        name: "root".into(),
        ..Default::default()
    };
    let mut stack: Vec<FbxNode> = vec![];

    loop {
        match parser
            .next_event()
            .map_err(|e| anyhow::anyhow!("FBX parse: {:?}", e))?
        {
            Event::StartNode(start) => {
                let name = start.name().to_string();
                let mut attrs_reader = start.attributes();
                let mut attrs = Vec::new();
                while let Some(attr) = attrs_reader
                    .load_next(DirectLoader)
                    .map_err(|e| anyhow::anyhow!("FBX attr: {:?}", e))?
                {
                    attrs.push(attr);
                }
                stack.push(FbxNode {
                    name,
                    attributes: attrs,
                    children: vec![],
                });
            }
            Event::EndNode => {
                if let Some(node) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else {
                        root.children.push(node);
                    }
                }
            }
            Event::EndFbx(_) => break,
        }
    }

    Ok(root)
}

// ═══════════════════════════════════════════════════════════════
// Geometry extraction
// ═══════════════════════════════════════════════════════════════

fn extract_geometry(tree: &FbxNode) -> Result<(Vec<f64>, Vec<f64>, Vec<i32>)> {
    let objects = tree
        .child("Objects")
        .ok_or_else(|| anyhow::anyhow!("No Objects node"))?;

    for child in &objects.children {
        if child.name == "Geometry" {
            let class = child.attr_str(2).unwrap_or("");
            if class == "Mesh" {
                let verts = child
                    .child("Vertices")
                    .and_then(|n| n.attr_f64_arr(0))
                    .unwrap_or(&[]);

                let indices = child
                    .child("PolygonVertexIndex")
                    .and_then(|n| n.attr_i32_arr(0))
                    .unwrap_or(&[]);

                // Normals — FBX stores in LayerElementNormal/Normals
                let normals = child
                    .child("LayerElementNormal")
                    .and_then(|n| n.child("Normals"))
                    .and_then(|n| n.attr_f64_arr(0))
                    .unwrap_or(&[]);

                return Ok((verts.to_vec(), normals.to_vec(), indices.to_vec()));
            }
        }
    }

    Err(anyhow::anyhow!("No Geometry/Mesh found in FBX"))
}

/// Build 24-byte stride vertex buffer: pos3 + normal3 (f32 each), triangulated.
/// Returns (bytes, tri_to_control_point_index) for skin weight expansion.
fn build_mesh_bytes(vertices: &[f64], normals: &[f64], indices: &[i32]) -> (Vec<u8>, Vec<usize>) {
    // FBX polygon indices: negative value marks end of polygon (bitwise NOT)
    let mut poly = Vec::new();

    // FBX normals can be ByPolygonVertex (indexed by polygon-vertex order)
    // or ByControlPoint (indexed by vertex index)
    let normals_by_polygon_vertex = normals.len() != vertices.len() && !normals.is_empty();
    let mut poly_vert_idx = 0usize;

    struct TriVert {
        pos_idx: usize,
        normal_idx: usize,
    }

    let mut tri_verts: Vec<TriVert> = Vec::new();

    for &idx in indices {
        let actual_idx = if idx < 0 { -(idx + 1) } else { idx };
        poly.push((actual_idx as usize, poly_vert_idx));
        poly_vert_idx += 1;

        if idx < 0 {
            // End of polygon — triangulate with fan
            for i in 1..poly.len() - 1 {
                tri_verts.push(TriVert {
                    pos_idx: poly[0].0,
                    normal_idx: poly[0].1,
                });
                tri_verts.push(TriVert {
                    pos_idx: poly[i].0,
                    normal_idx: poly[i].1,
                });
                tri_verts.push(TriVert {
                    pos_idx: poly[i + 1].0,
                    normal_idx: poly[i + 1].1,
                });
            }
            poly.clear();
        }
    }

    let vert_count = vertices.len() / 3;
    let normal_count = normals.len() / 3;

    let mut bytes = Vec::with_capacity(tri_verts.len() * 24);
    let mut tri_to_control: Vec<usize> = Vec::with_capacity(tri_verts.len());
    for tv in &tri_verts {
        if tv.pos_idx < vert_count {
            tri_to_control.push(tv.pos_idx);

            let px = vertices[tv.pos_idx * 3] as f32;
            let py = vertices[tv.pos_idx * 3 + 1] as f32;
            let pz = vertices[tv.pos_idx * 3 + 2] as f32;
            bytes.extend_from_slice(&px.to_le_bytes());
            bytes.extend_from_slice(&py.to_le_bytes());
            bytes.extend_from_slice(&pz.to_le_bytes());

            // Normal
            let ni = if normals_by_polygon_vertex {
                tv.normal_idx
            } else {
                tv.pos_idx
            };
            if ni < normal_count {
                let nx = normals[ni * 3] as f32;
                let ny = normals[ni * 3 + 1] as f32;
                let nz = normals[ni * 3 + 2] as f32;
                bytes.extend_from_slice(&nx.to_le_bytes());
                bytes.extend_from_slice(&ny.to_le_bytes());
                bytes.extend_from_slice(&nz.to_le_bytes());
            } else {
                bytes.extend_from_slice(&[0u8; 12]);
            }
        }
    }

    (bytes, tri_to_control)
}

// ═══════════════════════════════════════════════════════════════
// Skeleton extraction
// ═══════════════════════════════════════════════════════════════

/// Returns (skeleton_json, bone_names, pre_rotations_deg_per_bone)
fn extract_skeleton(tree: &FbxNode) -> Result<(Value, Vec<String>, Vec<[f64; 3]>)> {
    let objects = tree
        .child("Objects")
        .ok_or_else(|| anyhow::anyhow!("No Objects node"))?;

    let connections = tree
        .child("Connections")
        .ok_or_else(|| anyhow::anyhow!("No Connections node"))?;

    // Collect all Model nodes that are LimbNode or Null (skeleton bones)
    // Also extract their Lcl Translation/Rotation/Scale properties
    struct BoneInfo {
        id: i64,
        name: String,
        lcl_translation: [f64; 3],
        pre_rotation: [f64; 3],    // Euler degrees — applied BEFORE Lcl Rotation
        lcl_rotation: [f64; 3],    // Euler degrees
        lcl_scaling: [f64; 3],
    }

    let mut bones: Vec<BoneInfo> = Vec::new();
    let mut bone_ids: HashMap<i64, usize> = HashMap::new();

    for child in &objects.children {
        if child.name == "Model" {
            let class = child.attr_str(2).unwrap_or("");
            if class == "LimbNode" || class == "Null" || class == "Root" {
                let id = child.attr_i64(0).unwrap_or(0);
                let name = child
                    .attr_str(1)
                    .unwrap_or("bone")
                    .split('\0')
                    .next()
                    .unwrap_or("bone")
                    .to_string();
                let name = name.strip_prefix("Model::").unwrap_or(&name).to_string();

                // Extract local bind pose from Properties70
                let mut lcl_t = [0.0f64; 3];
                let mut pre_r = [0.0f64; 3];
                let mut lcl_r = [0.0f64; 3];
                let mut lcl_s = [1.0f64, 1.0, 1.0];

                if let Some(props) = child.child("Properties70") {
                    for p in &props.children {
                        if p.name == "P" {
                            let prop_name = p.attr_str(0).unwrap_or("");
                            match prop_name {
                                "Lcl Translation" => lcl_t = extract_p_xyz(p),
                                "PreRotation" => pre_r = extract_p_xyz(p),
                                "Lcl Rotation" => lcl_r = extract_p_xyz(p),
                                "Lcl Scaling" => lcl_s = extract_p_xyz(p),
                                _ => {}
                            }
                        }
                    }
                }

                bone_ids.insert(id, bones.len());
                bones.push(BoneInfo {
                    id,
                    name,
                    lcl_translation: lcl_t,
                    pre_rotation: pre_r,
                    lcl_rotation: lcl_r,
                    lcl_scaling: lcl_s,
                });
            }
        }
    }

    // Build parent-child relationships from Connections (OO = object-object)
    let mut parent_map: HashMap<usize, usize> = HashMap::new();
    for conn in &connections.children {
        if conn.name == "C" {
            let conn_type = conn.attr_str(0).unwrap_or("");
            if conn_type == "OO" {
                let child_id = conn.attr_i64(1).unwrap_or(0);
                let parent_id = conn.attr_i64(2).unwrap_or(0);
                if let (Some(&ci), Some(&pi)) =
                    (bone_ids.get(&child_id), bone_ids.get(&parent_id))
                {
                    parent_map.insert(ci, pi);
                }
            }
        }
    }

    let bone_names: Vec<String> = bones.iter().map(|b| b.name.clone()).collect();
    let pre_rotations: Vec<[f64; 3]> = bones.iter().map(|b| b.pre_rotation).collect();
    let mut bone_array = Vec::new();
    for (i, bone) in bones.iter().enumerate() {
        let parent = parent_map.get(&i).map(|&p| p as i64).unwrap_or(-1);

        // Build localBindTransform: T × PreRotation × LclRotation × S
        let local_mat = trs_pre_rot_mat4(
            bone.lcl_translation,
            bone.pre_rotation,
            bone.lcl_rotation,
            bone.lcl_scaling,
        );
        let mat_json: Vec<Value> = local_mat.iter().map(|&v| json!(v)).collect();

        bone_array.push(json!({
            "name": bone.name,
            "parent": parent,
            "index": i,
            "localBindTransform": mat_json,
        }));
    }

    let skeleton = json!({
        "bones": bone_array,
        "boneCount": bones.len(),
    });

    Ok((skeleton, bone_names, pre_rotations))
}

/// Extract x,y,z from FBX Properties70 P node (attributes at indices 4,5,6)
fn extract_p_xyz(p: &FbxNode) -> [f64; 3] {
    let x = match p.attributes.get(4) {
        Some(AttributeValue::F64(v)) => *v,
        Some(AttributeValue::F32(v)) => *v as f64,
        Some(AttributeValue::I32(v)) => *v as f64,
        _ => 0.0,
    };
    let y = match p.attributes.get(5) {
        Some(AttributeValue::F64(v)) => *v,
        Some(AttributeValue::F32(v)) => *v as f64,
        Some(AttributeValue::I32(v)) => *v as f64,
        _ => 0.0,
    };
    let z = match p.attributes.get(6) {
        Some(AttributeValue::F64(v)) => *v,
        Some(AttributeValue::F32(v)) => *v as f64,
        Some(AttributeValue::I32(v)) => *v as f64,
        _ => 0.0,
    };
    [x, y, z]
}

/// Build column-major 4x4 from T × PreRotation × LclRotation × S
fn trs_pre_rot_mat4(t: [f64; 3], pre_deg: [f64; 3], lcl_deg: [f64; 3], s: [f64; 3]) -> [f64; 16] {
    // Compose PreRotation and LclRotation as quaternions
    let pre_q = euler_xyz_to_quat(
        pre_deg[0].to_radians(),
        pre_deg[1].to_radians(),
        pre_deg[2].to_radians(),
    );
    let lcl_q = euler_xyz_to_quat(
        lcl_deg[0].to_radians(),
        lcl_deg[1].to_radians(),
        lcl_deg[2].to_radians(),
    );
    let combined_q = quat_mul_f64(pre_q, lcl_q);

    // Build rotation matrix from combined quaternion
    let [qx, qy, qz, qw] = combined_q;
    let xx = qx * qx; let yy = qy * qy; let zz = qz * qz;
    let xy = qx * qy; let xz = qx * qz; let yz = qy * qz;
    let wx = qw * qx; let wy = qw * qy; let wz = qw * qz;

    let r00 = 1.0 - 2.0 * (yy + zz);
    let r01 = 2.0 * (xy - wz);
    let r02 = 2.0 * (xz + wy);
    let r10 = 2.0 * (xy + wz);
    let r11 = 1.0 - 2.0 * (xx + zz);
    let r12 = 2.0 * (yz - wx);
    let r20 = 2.0 * (xz - wy);
    let r21 = 2.0 * (yz + wx);
    let r22 = 1.0 - 2.0 * (xx + yy);

    // Column-major: col0, col1, col2, col3
    [
        r00 * s[0], r10 * s[0], r20 * s[0], 0.0,
        r01 * s[1], r11 * s[1], r21 * s[1], 0.0,
        r02 * s[2], r12 * s[2], r22 * s[2], 0.0,
        t[0],       t[1],       t[2],       1.0,
    ]
}

/// Build column-major 4x4 matrix from TRS (Euler rotation in degrees, XYZ order)
fn trs_to_column_major_mat4(t: [f64; 3], r_deg: [f64; 3], s: [f64; 3]) -> [f64; 16] {
    let rx = r_deg[0].to_radians();
    let ry = r_deg[1].to_radians();
    let rz = r_deg[2].to_radians();

    let (sx, cx) = (rx.sin(), rx.cos());
    let (sy, cy) = (ry.sin(), ry.cos());
    let (sz, cz) = (rz.sin(), rz.cos());

    // Rotation = Rz * Ry * Rx (FBX convention)
    let r00 = cy * cz;
    let r01 = cz * sx * sy - cx * sz;
    let r02 = cx * cz * sy + sx * sz;
    let r10 = cy * sz;
    let r11 = cx * cz + sx * sy * sz;
    let r12 = cx * sy * sz - cz * sx;
    let r20 = -sy;
    let r21 = cy * sx;
    let r22 = cx * cy;

    // Column-major: [col0, col1, col2, col3]
    [
        r00 * s[0], r10 * s[0], r20 * s[0], 0.0,  // col 0
        r01 * s[1], r11 * s[1], r21 * s[1], 0.0,  // col 1
        r02 * s[2], r12 * s[2], r22 * s[2], 0.0,  // col 2
        t[0],       t[1],       t[2],       1.0,   // col 3
    ]
}

// ═══════════════════════════════════════════════════════════════
// Animation extraction
// ═══════════════════════════════════════════════════════════════

/// FBX time unit: 46186158000 ticks per second
const FBX_TIME_UNIT: f64 = 46186158000.0;

fn extract_animation(tree: &FbxNode, bone_names: &[String], pre_rotations: &[[f64; 3]]) -> Result<Value> {
    let objects = tree
        .child("Objects")
        .ok_or_else(|| anyhow::anyhow!("No Objects node"))?;
    let connections = tree
        .child("Connections")
        .ok_or_else(|| anyhow::anyhow!("No Connections node"))?;

    // Build connection graph: child_id → parent_id
    let mut conn_oo: Vec<(i64, i64)> = Vec::new(); // child → parent
    let mut conn_op: Vec<(i64, i64, String)> = Vec::new(); // child → parent, property
    for conn in &connections.children {
        if conn.name == "C" {
            let ct = conn.attr_str(0).unwrap_or("");
            let child_id = conn.attr_i64(1).unwrap_or(0);
            let parent_id = conn.attr_i64(2).unwrap_or(0);
            if ct == "OO" {
                conn_oo.push((child_id, parent_id));
            } else if ct == "OP" {
                let prop = conn.attr_str(3).unwrap_or("").to_string();
                conn_op.push((child_id, parent_id, prop));
            }
        }
    }

    // Collect AnimationCurveNodes: id → property name (T, R, S)
    let mut curve_node_prop: HashMap<i64, String> = HashMap::new();
    // Collect AnimationCurves: id → (times_sec, values)
    let mut curves: HashMap<i64, (Vec<f64>, Vec<f32>)> = HashMap::new();
    // Collect Model ids → bone name
    let mut model_id_to_bone: HashMap<i64, String> = HashMap::new();

    for child in &objects.children {
        match child.name.as_str() {
            "AnimationCurveNode" => {
                let id = child.attr_i64(0).unwrap_or(0);
                let name = child
                    .attr_str(1)
                    .unwrap_or("")
                    .split('\0')
                    .next()
                    .unwrap_or("")
                    .to_string();
                // AnimationCurveNode names: "T" (translation), "R" (rotation), "S" (scale)
                // Sometimes prefixed like "AnimCurveNode::" — strip it
                let prop = name
                    .strip_prefix("AnimCurveNode::")
                    .unwrap_or(&name)
                    .to_string();
                curve_node_prop.insert(id, prop);
            }
            "AnimationCurve" => {
                let id = child.attr_i64(0).unwrap_or(0);
                let times: Vec<f64> = child
                    .child("KeyTime")
                    .and_then(|n| n.attr_i64_arr(0))
                    .map(|arr| arr.iter().map(|&t| t as f64 / FBX_TIME_UNIT).collect())
                    .unwrap_or_default();
                let values: Vec<f32> = child
                    .child("KeyValueFloat")
                    .and_then(|n| match n.attributes.first() {
                        Some(AttributeValue::ArrF32(arr)) => Some(arr.clone()),
                        Some(AttributeValue::ArrF64(arr)) => {
                            Some(arr.iter().map(|&v| v as f32).collect())
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                if !times.is_empty() && !values.is_empty() {
                    curves.insert(id, (times, values));
                }
            }
            "Model" => {
                let id = child.attr_i64(0).unwrap_or(0);
                let name = child
                    .attr_str(1)
                    .unwrap_or("")
                    .split('\0')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let name = name.strip_prefix("Model::").unwrap_or(&name).to_string();
                model_id_to_bone.insert(id, name);
            }
            _ => {}
        }
    }

    // Map: AnimationCurve → AnimationCurveNode (via OO connections)
    let mut curve_to_curvenode: HashMap<i64, i64> = HashMap::new();
    // Map: AnimationCurveNode → Model (via OO connections)
    let mut curvenode_to_model: HashMap<i64, i64> = HashMap::new();
    // Map: AnimationCurve → channel property (d|X, d|Y, d|Z via OP connections)
    let mut curve_channel: HashMap<i64, String> = HashMap::new();

    for &(child_id, parent_id) in &conn_oo {
        if curves.contains_key(&child_id) && curve_node_prop.contains_key(&parent_id) {
            curve_to_curvenode.insert(child_id, parent_id);
        }
        if curve_node_prop.contains_key(&child_id) && model_id_to_bone.contains_key(&parent_id) {
            curvenode_to_model.insert(child_id, parent_id);
        }
    }
    for (child_id, parent_id, prop) in &conn_op {
        if curves.contains_key(child_id) && curve_node_prop.contains_key(parent_id) {
            curve_to_curvenode.insert(*child_id, *parent_id);
            curve_channel.insert(*child_id, prop.clone());
        }
        if curve_node_prop.contains_key(child_id) && model_id_to_bone.contains_key(parent_id) {
            curvenode_to_model.insert(*child_id, *parent_id);
        }
    }

    // Collect per-scalar curve data
    struct ScalarCurve {
        bone_idx: usize,
        property: String, // "T", "R", "S"
        component: String, // "d|X", "d|Y", "d|Z"
        times: Vec<f64>,
        values: Vec<f32>,
    }

    let mut scalar_curves: Vec<ScalarCurve> = Vec::new();
    let mut max_time: f64 = 0.0;

    for (&curve_id, (times, values)) in &curves {
        let cn_id = match curve_to_curvenode.get(&curve_id) {
            Some(id) => *id,
            None => continue,
        };
        let model_id = match curvenode_to_model.get(&cn_id) {
            Some(id) => *id,
            None => continue,
        };
        let bone_name = match model_id_to_bone.get(&model_id) {
            Some(n) => n,
            None => continue,
        };
        let bone_idx = match bone_names.iter().position(|n| n == bone_name) {
            Some(i) => i,
            None => continue,
        };
        let property = curve_node_prop.get(&cn_id).cloned().unwrap_or_default();
        let component = curve_channel.get(&curve_id).cloned().unwrap_or_default();

        if let Some(&t) = times.last() {
            if t > max_time {
                max_time = t;
            }
        }

        scalar_curves.push(ScalarCurve {
            bone_idx,
            property,
            component,
            times: times.clone(),
            values: values.clone(),
        });
    }

    // Group by (bone_idx, property) and merge X/Y/Z into vec3 or quat channels
    // Key: (bone_idx, property) → { "d|X": curve, "d|Y": curve, "d|Z": curve }
    let mut grouped: HashMap<(usize, String), HashMap<String, (Vec<f64>, Vec<f32>)>> =
        HashMap::new();
    for sc in &scalar_curves {
        grouped
            .entry((sc.bone_idx, sc.property.clone()))
            .or_default()
            .insert(
                sc.component.clone(),
                (sc.times.clone(), sc.values.clone()),
            );
    }

    let mut channels: Vec<Value> = Vec::new();
    for ((bone_idx, prop), components) in &grouped {
        let property = match prop.as_str() {
            "T" => "position",
            "R" => "rotation",
            "S" => "scale",
            _ => continue,
        };

        let x_curve = components.get("d|X");
        let y_curve = components.get("d|Y");
        let z_curve = components.get("d|Z");

        // Use the longest time array as reference
        let ref_times = [x_curve, y_curve, z_curve]
            .iter()
            .filter_map(|c| c.map(|(t, _)| t))
            .max_by_key(|t| t.len())
            .cloned()
            .unwrap_or_default();

        if ref_times.is_empty() {
            continue;
        }

        let times_json: Vec<Value> = ref_times.iter().map(|&t| json!(t)).collect();

        // Sample each component at reference times
        let values_json: Vec<Value> = if property == "rotation" {
            // FBX: full rotation = PreRotation × LclRotation
            // Compose PreRotation quat with animated rotation quat
            let pre_r = if *bone_idx < pre_rotations.len() {
                pre_rotations[*bone_idx]
            } else {
                [0.0, 0.0, 0.0]
            };
            let pre_q = euler_xyz_to_quat(
                pre_r[0].to_radians(),
                pre_r[1].to_radians(),
                pre_r[2].to_radians(),
            );

            ref_times
                .iter()
                .map(|&t| {
                    let rx = sample_curve(x_curve, t).to_radians() as f64;
                    let ry = sample_curve(y_curve, t).to_radians() as f64;
                    let rz = sample_curve(z_curve, t).to_radians() as f64;
                    let anim_q = euler_xyz_to_quat(rx, ry, rz);
                    let q = quat_mul_f64(pre_q, anim_q);
                    json!([q[0], q[1], q[2], q[3]])
                })
                .collect()
        } else {
            // Position or scale: vec3
            ref_times
                .iter()
                .map(|&t| {
                    let x = sample_curve(x_curve, t);
                    let y = sample_curve(y_curve, t);
                    let z = sample_curve(z_curve, t);
                    json!([x, y, z])
                })
                .collect()
        };

        channels.push(json!({
            "boneIndex": bone_idx,
            "property": property,
            "interpolation": "linear",
            "times": times_json,
            "values": values_json,
        }));
    }

    let clip = json!({
        "name": "mixamo_clip",
        "duration": max_time,
        "channels": channels,
        "boneCount": bone_names.len(),
    });

    Ok(clip)
}

/// Sample a scalar animation curve at time t (linear interpolation)
fn sample_curve(curve: Option<&(Vec<f64>, Vec<f32>)>, t: f64) -> f32 {
    let (times, values) = match curve {
        Some(c) if !c.0.is_empty() => (&c.0, &c.1),
        _ => return 0.0,
    };
    if times.len() == 1 || t <= times[0] {
        return values[0];
    }
    if t >= *times.last().unwrap() {
        return *values.last().unwrap();
    }
    // Find bracket
    let mut i = 0;
    while i + 1 < times.len() && times[i + 1] < t {
        i += 1;
    }
    let dt = times[i + 1] - times[i];
    if dt <= 0.0 {
        return values[i];
    }
    let frac = ((t - times[i]) / dt) as f32;
    values[i] * (1.0 - frac) + values[i + 1] * frac
}

/// Multiply two quaternions [x, y, z, w]: result = a * b
fn quat_mul_f64(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

/// Convert Euler XYZ angles (radians) to quaternion [x, y, z, w]
fn euler_xyz_to_quat(rx: f64, ry: f64, rz: f64) -> [f64; 4] {
    let (sx, cx) = (rx * 0.5).sin_cos();
    let (sy, cy) = (ry * 0.5).sin_cos();
    let (sz, cz) = (rz * 0.5).sin_cos();

    // ZYX composition order (FBX convention: Rz * Ry * Rx)
    let w = cx * cy * cz + sx * sy * sz;
    let x = sx * cy * cz - cx * sy * sz;
    let y = cx * sy * cz + sx * cy * sz;
    let z = cx * cy * sz - sx * sy * cz;

    [x, y, z, w]
}

// ═══════════════════════════════════════════════════════════════
// Skin weight extraction from Deformer/Cluster nodes
// ═══════════════════════════════════════════════════════════════

fn extract_skin_weights(
    tree: &FbxNode,
    vertex_count: usize,
    bone_count: usize,
) -> Result<(Vec<u8>, Value)> {
    let max_influences = 4;
    let objects = tree.child("Objects");
    let connections = tree.child("Connections");

    // Per-vertex: collect (bone_idx, weight) pairs
    let mut vert_weights: Vec<Vec<(u16, f32)>> = vec![Vec::new(); vertex_count];

    if let (Some(objects), Some(connections)) = (objects, connections) {
        // Build OO connection map for Deformer lookup
        let mut conn_oo: Vec<(i64, i64)> = Vec::new();
        for conn in &connections.children {
            if conn.name == "C" && conn.attr_str(0) == Some("OO") {
                let child_id = conn.attr_i64(1).unwrap_or(0);
                let parent_id = conn.attr_i64(2).unwrap_or(0);
                conn_oo.push((child_id, parent_id));
            }
        }

        // Collect SubDeformer (Cluster) nodes — these hold per-bone skin weights
        // Each Cluster has: Indexes (vertex indices), Weights (per-vertex weights),
        // Transform, TransformLink matrices
        let mut cluster_to_bone: HashMap<i64, usize> = HashMap::new();

        // Find Model IDs that are bones
        let mut model_name_to_idx: HashMap<String, usize> = HashMap::new();
        let mut model_ids: HashMap<i64, String> = HashMap::new();
        let mut bone_idx = 0usize;
        for child in &objects.children {
            if child.name == "Model" {
                let class = child.attr_str(2).unwrap_or("");
                if class == "LimbNode" || class == "Null" || class == "Root" {
                    let id = child.attr_i64(0).unwrap_or(0);
                    let name = child
                        .attr_str(1)
                        .unwrap_or("bone")
                        .split('\0')
                        .next()
                        .unwrap_or("bone")
                        .to_string();
                    let name = name.strip_prefix("Model::").unwrap_or(&name).to_string();
                    model_ids.insert(id, name.clone());
                    model_name_to_idx.insert(name, bone_idx);
                    bone_idx += 1;
                }
            }
        }

        // Collect all Cluster IDs
        let cluster_ids: Vec<i64> = objects.children.iter()
            .filter(|c| c.name == "Deformer" && c.attr_str(2) == Some("Cluster"))
            .filter_map(|c| c.attr_i64(0))
            .collect();

        // Map Cluster → Model (bone) via connections
        // FBX connection: Model(bone) is CHILD, Cluster is PARENT
        let cluster_id_set: std::collections::HashSet<i64> = cluster_ids.iter().copied().collect();
        for &(child_id, parent_id) in &conn_oo {
            if let Some(bone_name) = model_ids.get(&child_id) {
                if cluster_id_set.contains(&parent_id) {
                    if let Some(&bi) = model_name_to_idx.get(bone_name) {
                        cluster_to_bone.insert(parent_id, bi);
                    }
                }
            }
        }

        // Extract weights from each Cluster
        for child in &objects.children {
            if child.name == "Deformer" && child.attr_str(2) == Some("Cluster") {
                let cluster_id = child.attr_i64(0).unwrap_or(0);
                let bi = match cluster_to_bone.get(&cluster_id) {
                    Some(&b) => b,
                    None => continue,
                };

                let vert_indices = child
                    .child("Indexes")
                    .and_then(|n| n.attr_i32_arr(0))
                    .unwrap_or(&[]);
                let weights = child
                    .child("Weights")
                    .and_then(|n| n.attr_f64_arr(0))
                    .unwrap_or(&[]);

                for (i, &vi) in vert_indices.iter().enumerate() {
                    let vi = vi as usize;
                    if vi < vertex_count {
                        let w = weights.get(i).copied().unwrap_or(0.0) as f32;
                        if w > 0.0 {
                            vert_weights[vi].push((bi as u16, w));
                        }
                    }
                }
            }
        }
    }

    // Merge duplicate bone entries, normalize, and truncate to max_influences
    let mut skin_bytes = Vec::with_capacity(vertex_count * max_influences * 6);
    for vw in &mut vert_weights {
        // Merge weights for the same bone index
        let mut merged: HashMap<u16, f32> = HashMap::new();
        for &(bi, w) in vw.iter() {
            *merged.entry(bi).or_insert(0.0) += w;
        }
        *vw = merged.into_iter().collect();

        // Sort by weight descending, keep top max_influences
        vw.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        vw.truncate(max_influences);

        // Normalize weights
        let total: f32 = vw.iter().map(|(_, w)| w).sum();
        if total > 0.0 {
            for (_, w) in vw.iter_mut() {
                *w /= total;
            }
        }

        for j in 0..max_influences {
            let (bone_idx, weight) = vw.get(j).copied().unwrap_or((0, 0.0));
            skin_bytes.extend_from_slice(&bone_idx.to_le_bytes());
            skin_bytes.extend_from_slice(&weight.to_le_bytes());
        }
    }

    let desc = json!({
        "vertexCount": vertex_count,
        "maxInfluences": max_influences,
        "boneCount": bone_count,
    });

    Ok((skin_bytes, desc))
}

// ═══════════════════════════════════════════════════════════════
// Inverse bind matrices from Cluster Transform/TransformLink
// ═══════════════════════════════════════════════════════════════

fn extract_inverse_bind_matrices(tree: &FbxNode, bone_count: usize) -> Result<Vec<u8>> {
    let objects = tree.child("Objects");
    let connections = tree.child("Connections");

    // Default: identity matrices
    let identity: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let mut matrices: Vec<[f32; 16]> = vec![identity; bone_count];

    if let (Some(objects), Some(connections)) = (objects, connections) {
        // Same cluster-to-bone mapping as skin weights
        let mut conn_oo: Vec<(i64, i64)> = Vec::new();
        for conn in &connections.children {
            if conn.name == "C" && conn.attr_str(0) == Some("OO") {
                conn_oo.push((
                    conn.attr_i64(1).unwrap_or(0),
                    conn.attr_i64(2).unwrap_or(0),
                ));
            }
        }

        let mut model_ids: HashMap<i64, usize> = HashMap::new();
        let mut bone_idx = 0usize;
        for child in &objects.children {
            if child.name == "Model" {
                let class = child.attr_str(2).unwrap_or("");
                if class == "LimbNode" || class == "Null" || class == "Root" {
                    let id = child.attr_i64(0).unwrap_or(0);
                    model_ids.insert(id, bone_idx);
                    bone_idx += 1;
                }
            }
        }

        // Collect Cluster IDs and map to bones (Model is CHILD, Cluster is PARENT in FBX connections)
        let cluster_id_set: std::collections::HashSet<i64> = objects.children.iter()
            .filter(|c| c.name == "Deformer" && c.attr_str(2) == Some("Cluster"))
            .filter_map(|c| c.attr_i64(0))
            .collect();

        let mut cluster_to_bone: HashMap<i64, usize> = HashMap::new();
        for &(child_id, parent_id) in &conn_oo {
            if let Some(&bi) = model_ids.get(&child_id) {
                if cluster_id_set.contains(&parent_id) {
                    cluster_to_bone.insert(parent_id, bi);
                }
            }
        }

        // Extract TransformLink (inverse bind pose) from each Cluster
        for child in &objects.children {
            if child.name == "Deformer" && child.attr_str(2) == Some("Cluster") {
                let cluster_id = child.attr_i64(0).unwrap_or(0);
                let bi = match cluster_to_bone.get(&cluster_id) {
                    Some(&b) => b,
                    None => continue,
                };

                // TransformLink is the bone's global bind pose transform
                // The inverse bind matrix = inverse(TransformLink)
                if let Some(tl) = child.child("TransformLink") {
                    if let Some(arr) = tl.attr_f64_arr(0) {
                        if arr.len() >= 16 {
                            // FBX row-major layout matches our flat column-major convention
                            // (both put translation at indices 12,13,14)
                            let mut m = [0f32; 16];
                            for i in 0..16 {
                                m[i] = arr[i] as f32;
                            }
                            if let Some(inv) = invert_4x4(&m) {
                                matrices[bi] = inv;
                            }
                        }
                    }
                }
            }
        }
    }

    let mut bytes = Vec::with_capacity(bone_count * 64);
    for mat in &matrices {
        for &v in mat {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    Ok(bytes)
}

/// Invert a 4x4 column-major matrix
fn invert_4x4(m: &[f32; 16]) -> Option<[f32; 16]> {
    let mut inv = [0f32; 16];

    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
        + m[9] * m[7] * m[14]
        + m[13] * m[6] * m[11]
        - m[13] * m[7] * m[10];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
        - m[8] * m[7] * m[14]
        - m[12] * m[6] * m[11]
        + m[12] * m[7] * m[10];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
        + m[8] * m[7] * m[13]
        + m[12] * m[5] * m[11]
        - m[12] * m[7] * m[9];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
        - m[8] * m[6] * m[13]
        - m[12] * m[5] * m[10]
        + m[12] * m[6] * m[9];
    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
        - m[9] * m[3] * m[14]
        - m[13] * m[2] * m[11]
        + m[13] * m[3] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
        + m[8] * m[3] * m[14]
        + m[12] * m[2] * m[11]
        - m[12] * m[3] * m[10];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
        - m[8] * m[3] * m[13]
        - m[12] * m[1] * m[11]
        + m[12] * m[3] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
        + m[8] * m[2] * m[13]
        + m[12] * m[1] * m[10]
        - m[12] * m[2] * m[9];
    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
        + m[5] * m[3] * m[14]
        + m[13] * m[2] * m[7]
        - m[13] * m[3] * m[6];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
        - m[4] * m[3] * m[14]
        - m[12] * m[2] * m[7]
        + m[12] * m[3] * m[6];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
        + m[4] * m[3] * m[13]
        + m[12] * m[1] * m[7]
        - m[12] * m[3] * m[5];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
        - m[4] * m[2] * m[13]
        - m[12] * m[1] * m[6]
        + m[12] * m[2] * m[5];
    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
        - m[5] * m[3] * m[10]
        - m[9] * m[2] * m[7]
        + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
        + m[4] * m[3] * m[10]
        + m[8] * m[2] * m[7]
        - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
        - m[4] * m[3] * m[9]
        - m[8] * m[1] * m[7]
        + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
        + m[4] * m[2] * m[9]
        + m[8] * m[1] * m[6]
        - m[8] * m[2] * m[5];

    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-10 {
        return None;
    }
    let inv_det = 1.0 / det;
    for v in &mut inv {
        *v *= inv_det;
    }
    Some(inv)
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mixamo_fbx() {
        let data = std::fs::read("../../assets/leg_sweep.fbx")
            .expect("Failed to read leg_sweep.fbx — run from crate root");
        println!("FBX file size: {} bytes", data.len());

        let result = import_fbx(&data);
        match &result {
            Ok(out) => {
                assert!(out.contains_key("mesh"), "Should have mesh");
                assert!(out.contains_key("skeleton"), "Should have skeleton");
                assert!(out.contains_key("clip"), "Should have clip");
                assert!(out.contains_key("metadata"), "Should have metadata");

                if let Some(Message::Object(meta)) = out.get("metadata") {
                    let v: Value = meta.as_ref().clone().into();
                    println!(
                        "Metadata: {}",
                        serde_json::to_string_pretty(&v).unwrap()
                    );
                    let verts = v["vertices"].as_u64().unwrap_or(0);
                    let bones = v["bones"].as_u64().unwrap_or(0);
                    assert!(verts > 0, "Should have vertices");
                    assert!(bones > 0, "Should have bones");
                    println!("Vertices: {}, Bones: {}", verts, bones);
                    if let Some(names) = v["boneNames"].as_array() {
                        println!("Bone hierarchy ({} bones):", names.len());
                        for n in names {
                            println!("  {}", n);
                        }
                    }
                }

                if let Some(Message::Bytes(mesh)) = out.get("mesh") {
                    let tri_verts = mesh.len() / 24;
                    println!(
                        "Mesh: {} bytes, {} triangle-vertices, {} triangles",
                        mesh.len(),
                        tri_verts,
                        tri_verts / 3
                    );
                    // Print vertex bounds
                    let mut min = [f32::MAX; 3];
                    let mut max = [f32::MIN; 3];
                    for i in 0..tri_verts {
                        let off = i * 24;
                        for j in 0..3 {
                            let v = f32::from_le_bytes([
                                mesh[off + j * 4],
                                mesh[off + j * 4 + 1],
                                mesh[off + j * 4 + 2],
                                mesh[off + j * 4 + 3],
                            ]);
                            if v < min[j] { min[j] = v; }
                            if v > max[j] { max[j] = v; }
                        }
                    }
                    println!(
                        "Bounds: x=[{:.1}, {:.1}] y=[{:.1}, {:.1}] z=[{:.1}, {:.1}]",
                        min[0], max[0], min[1], max[1], min[2], max[2]
                    );
                }

                if let Some(Message::Object(clip)) = out.get("clip") {
                    let v: Value = clip.as_ref().clone().into();
                    let duration = v["duration"].as_f64().unwrap_or(0.0);
                    let channels = v["channels"].as_array().map(|a| a.len()).unwrap_or(0);
                    println!(
                        "Animation: duration={:.3}s, {} channels",
                        duration, channels
                    );
                    if let Some(chs) = v["channels"].as_array() {
                        for ch in chs.iter().take(8) {
                            let bi = ch["boneIndex"].as_u64().unwrap_or(0);
                            let prop = ch["property"].as_str().unwrap_or("?");
                            let kf_count = ch["times"]
                                .as_array()
                                .map(|a| a.len())
                                .unwrap_or(0);
                            println!(
                                "  bone[{}] {}: {} keyframes",
                                bi, prop, kf_count
                            );
                            // Print first value
                            if let Some(vals) = ch["values"].as_array() {
                                if let Some(v0) = vals.first() {
                                    println!("    first value: {}", v0);
                                }
                            }
                        }
                        if chs.len() > 8 {
                            println!("  ... and {} more channels", chs.len() - 8);
                        }
                    }
                }

                if let Some(Message::Object(sd)) = out.get("skin_descriptor") {
                    let v: Value = sd.as_ref().clone().into();
                    println!("Skin: {}", serde_json::to_string(&v).unwrap());
                }

                // Print skin weight samples from various body parts
                if let Some(Message::Bytes(skin)) = out.get("skin") {
                    let stride = 24; // 4 influences × 6 bytes
                    let total = skin.len() / stride;
                    println!("Skin weight samples ({} total):", total);
                    for &vi in &[0, 100, total/4, total/2, total*3/4, total-1] {
                        if vi >= total { continue; }
                        let off = vi * stride;
                        print!("  v[{}]: ", vi);
                        for j in 0..4 {
                            let w_off = off + j * 6;
                            let bi = u16::from_le_bytes([skin[w_off], skin[w_off + 1]]);
                            let w = f32::from_le_bytes(skin[w_off + 2..w_off + 6].try_into().unwrap());
                            if w > 0.001 { print!("b{}={:.3} ", bi, w); }
                        }
                        println!();
                    }
                }
            }
            Err(e) => {
                panic!("FBX import failed: {}", e);
            }
        }
    }
}
