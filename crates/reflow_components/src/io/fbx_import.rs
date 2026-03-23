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
    let mesh_bytes = build_mesh_bytes(&vertices, &normals, &indices);
    out.insert("mesh".to_string(), Message::bytes(mesh_bytes));

    // Extract skeleton hierarchy
    let (skeleton, bone_names) = extract_skeleton(&tree)?;
    out.insert(
        "skeleton".to_string(),
        Message::object(EncodableValue::from(skeleton.clone())),
    );

    // Extract animation curves
    let clip = extract_animation(&tree, &bone_names)?;
    out.insert(
        "clip".to_string(),
        Message::object(EncodableValue::from(clip)),
    );

    // Extract skin weights from Deformer/Cluster nodes
    let (skin_bytes, skin_desc) =
        extract_skin_weights(&tree, vertices.len() / 3, bone_names.len())?;
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
fn build_mesh_bytes(vertices: &[f64], normals: &[f64], indices: &[i32]) -> Vec<u8> {
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
    for tv in &tri_verts {
        if tv.pos_idx < vert_count {
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

    bytes
}

// ═══════════════════════════════════════════════════════════════
// Skeleton extraction
// ═══════════════════════════════════════════════════════════════

fn extract_skeleton(tree: &FbxNode) -> Result<(Value, Vec<String>)> {
    let objects = tree
        .child("Objects")
        .ok_or_else(|| anyhow::anyhow!("No Objects node"))?;

    let connections = tree
        .child("Connections")
        .ok_or_else(|| anyhow::anyhow!("No Connections node"))?;

    // Collect all Model nodes that are LimbNode or Null (skeleton bones)
    let mut bones: Vec<(i64, String)> = Vec::new();
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
                // Strip "Model::" prefix if present
                let name = name.strip_prefix("Model::").unwrap_or(&name).to_string();
                bone_ids.insert(id, bones.len());
                bones.push((id, name));
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

    let bone_names: Vec<String> = bones.iter().map(|(_, n)| n.clone()).collect();
    let mut bone_array = Vec::new();
    for (i, (_, name)) in bones.iter().enumerate() {
        let parent = parent_map.get(&i).map(|&p| p as i64).unwrap_or(-1);
        bone_array.push(json!({
            "name": name,
            "parent": parent,
        }));
    }

    let skeleton = json!({
        "bones": bone_array,
        "boneCount": bones.len(),
    });

    Ok((skeleton, bone_names))
}

// ═══════════════════════════════════════════════════════════════
// Animation extraction
// ═══════════════════════════════════════════════════════════════

/// FBX time unit: 46186158000 ticks per second
const FBX_TIME_UNIT: f64 = 46186158000.0;

fn extract_animation(tree: &FbxNode, bone_names: &[String]) -> Result<Value> {
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

    // Build animation channels: bone_name + property + component → keyframes
    // Group: (bone_name, property) → { x: curve, y: curve, z: curve }
    struct CurveData {
        bone_idx: usize,
        property: String, // "T", "R", "S"
        component: String, // "d|X", "d|Y", "d|Z"
        times: Vec<f64>,
        values: Vec<f32>,
    }

    let mut curve_list: Vec<CurveData> = Vec::new();
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

        curve_list.push(CurveData {
            bone_idx,
            property,
            component,
            times: times.clone(),
            values: values.clone(),
        });
    }

    // Group by (bone_idx, property) and build JSON channels
    let mut channels: Vec<Value> = Vec::new();
    for cd in &curve_list {
        let path = match cd.property.as_str() {
            "T" => "translation",
            "R" => "rotation",
            "S" => "scale",
            _ => &cd.property,
        };
        let keyframes: Vec<Value> = cd
            .times
            .iter()
            .zip(cd.values.iter())
            .map(|(&t, &v)| json!({ "time": t, "value": v }))
            .collect();

        channels.push(json!({
            "boneIndex": cd.bone_idx,
            "path": path,
            "component": cd.component,
            "keyframes": keyframes,
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

        // Map Cluster → Model (bone) via connections: Cluster → Deformer → Model
        // Actually: Cluster (SubDeformer) is connected OO to Model (the bone it deforms toward)
        for &(child_id, parent_id) in &conn_oo {
            // Check if child is a Cluster/SubDeformer
            if let Some(bone_name) = model_ids.get(&parent_id) {
                if let Some(&bi) = model_name_to_idx.get(bone_name) {
                    // Check if child_id is a SubDeformer node
                    for obj in &objects.children {
                        if obj.name == "Deformer"
                            && obj.attr_i64(0) == Some(child_id)
                            && obj.attr_str(2) == Some("Cluster")
                        {
                            cluster_to_bone.insert(child_id, bi);
                        }
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

    // Normalize and truncate to max_influences per vertex
    let mut skin_bytes = Vec::with_capacity(vertex_count * max_influences * 6);
    for vw in &mut vert_weights {
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

        let mut cluster_to_bone: HashMap<i64, usize> = HashMap::new();
        for &(child_id, parent_id) in &conn_oo {
            if let Some(&bi) = model_ids.get(&parent_id) {
                for obj in &objects.children {
                    if obj.name == "Deformer"
                        && obj.attr_i64(0) == Some(child_id)
                        && obj.attr_str(2) == Some("Cluster")
                    {
                        cluster_to_bone.insert(child_id, bi);
                    }
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

                // TransformLink is the bone's world transform at bind pose
                // The inverse bind matrix = inverse(TransformLink)
                if let Some(tl) = child.child("TransformLink") {
                    if let Some(arr) = tl.attr_f64_arr(0) {
                        if arr.len() >= 16 {
                            // FBX stores row-major 4x4, convert to column-major f32
                            let mut m = [0f32; 16];
                            for r in 0..4 {
                                for c in 0..4 {
                                    m[c * 4 + r] = arr[r * 4 + c] as f32;
                                }
                            }
                            // Invert the matrix
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
                }

                if let Some(Message::Object(clip)) = out.get("clip") {
                    let v: Value = clip.as_ref().clone().into();
                    let duration = v["duration"].as_f64().unwrap_or(0.0);
                    let channels = v["channels"].as_array().map(|a| a.len()).unwrap_or(0);
                    println!(
                        "Animation: duration={:.3}s, {} channels",
                        duration, channels
                    );
                    // Print first few channels
                    if let Some(chs) = v["channels"].as_array() {
                        for ch in chs.iter().take(5) {
                            let bi = ch["boneIndex"].as_u64().unwrap_or(0);
                            let path = ch["path"].as_str().unwrap_or("?");
                            let comp = ch["component"].as_str().unwrap_or("?");
                            let kf_count = ch["keyframes"]
                                .as_array()
                                .map(|a| a.len())
                                .unwrap_or(0);
                            println!(
                                "  bone[{}] {}.{}: {} keyframes",
                                bi, path, comp, kf_count
                            );
                        }
                        if chs.len() > 5 {
                            println!("  ... and {} more channels", chs.len() - 5);
                        }
                    }
                }

                if let Some(Message::Object(sd)) = out.get("skin_descriptor") {
                    let v: Value = sd.as_ref().clone().into();
                    println!("Skin: {}", serde_json::to_string(&v).unwrap());
                }
            }
            Err(e) => {
                panic!("FBX import failed: {}", e);
            }
        }
    }
}
