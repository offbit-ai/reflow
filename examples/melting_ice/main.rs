//! # Melting Ice Cube — SDF Ray March + Material Subgraphs
//!
//! This example keeps the flow-editor topology DAG-shaped:
//! - `ice_material` and `water_material` are separate shader subgraphs
//! - `ice_geometry` and `puddle_geometry` are separate SDF subgraphs
//! - the parent scene composes tagged geometry and dispatches by material slot

use anyhow::{Context, Result};
use reflow_network::{
    actor::Actor,
    connector::{ConnectionPoint, Connector, InitialPacket},
    graph::types::{GraphConnection, GraphEdge, GraphExport, GraphNode},
    message::Message,
    network::{Network, NetworkConfig},
    subgraph::SubgraphActor,
};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(default)
}

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
}

fn config(cfg: Value) -> Option<HashMap<String, Value>> {
    if let Value::Object(map) = cfg {
        Some(map.into_iter().collect())
    } else {
        None
    }
}

fn wire(fa: &str, fp: &str, ta: &str, tp: &str) -> Connector {
    Connector {
        from: ConnectionPoint {
            actor: fa.to_owned(),
            port: fp.to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: ta.to_owned(),
            port: tp.to_owned(),
            ..Default::default()
        },
    }
}

fn iip(node: &str, port: &str, msg: Message) -> InitialPacket {
    InitialPacket {
        to: ConnectionPoint::new(node, port, Some(msg)),
    }
}

fn sg_node(id: &str, component: &str, metadata: Option<HashMap<String, Value>>) -> (String, GraphNode) {
    (
        id.to_string(),
        GraphNode {
            id: id.to_string(),
            component: component.to_string(),
            metadata,
        },
    )
}

fn sg_edge(node_id: &str, port: &str) -> GraphEdge {
    GraphEdge {
        node_id: node_id.to_string(),
        port_name: port.to_string(),
        port_id: port.to_string(),
        ..Default::default()
    }
}

fn sg_conn(from_node: &str, from_port: &str, to_node: &str, to_port: &str) -> GraphConnection {
    GraphConnection {
        from: sg_edge(from_node, from_port),
        to: sg_edge(to_node, to_port),
        metadata: None,
        data: None,
    }
}

fn template_actors(templates: &[&str]) -> Result<HashMap<String, Arc<dyn Actor>>> {
    let mut actors = HashMap::new();
    for template in templates {
        let actor = reflow_components::get_actor_for_template(template)
            .with_context(|| format!("Missing actor template '{template}'"))?;
        actors.insert((*template).to_string(), actor);
    }
    Ok(actors)
}

#[derive(Clone, Copy)]
struct PuddleShapeSpec {
    radius: f32,
    height: f32,
    noise_freq: f32,
    noise_amp: f32,
    bevel: f32,
    meniscus: f32,
}

#[derive(Clone, Copy)]
struct PuddleLobeSpec {
    name: &'static str,
    shape: PuddleShapeSpec,
    scale: [f32; 3],
    rotate_y: Option<f32>,
    offset: [f32; 3],
}

fn add_process(
    processes: &mut HashMap<String, GraphNode>,
    id: &str,
    component: &str,
    metadata: Option<HashMap<String, Value>>,
) {
    let (node_id, node) = sg_node(id, component, metadata);
    processes.insert(node_id, node);
}

fn add_triggered_process(
    processes: &mut HashMap<String, GraphNode>,
    connections: &mut Vec<GraphConnection>,
    id: &str,
    component: &str,
    metadata: Option<HashMap<String, Value>>,
) {
    add_process(processes, id, component, metadata);
    connections.push(sg_conn("start", "output", id, "_trigger"));
}

fn add_sdf_binary_op(
    processes: &mut HashMap<String, GraphNode>,
    connections: &mut Vec<GraphConnection>,
    id: &str,
    component: &str,
    metadata: Option<HashMap<String, Value>>,
    left: &str,
    right: &str,
) {
    add_process(processes, id, component, metadata);
    connections.push(sg_conn(left, "sdf", id, "sdf_a"));
    connections.push(sg_conn(right, "sdf", id, "sdf_b"));
}

fn translate_metadata(offset: [f32; 3]) -> Option<HashMap<String, Value>> {
    config(json!({
        "x": offset[0],
        "y": offset[1],
        "z": offset[2],
    }))
}

fn scale_metadata(factor: [f32; 3]) -> Option<HashMap<String, Value>> {
    config(json!({
        "factorX": factor[0],
        "factorY": factor[1],
        "factorZ": factor[2],
    }))
}

fn rotate_y_metadata(angle: f32) -> Option<HashMap<String, Value>> {
    config(json!({ "y": angle }))
}

fn puddle_shape_metadata(shape: PuddleShapeSpec) -> Option<HashMap<String, Value>> {
    config(json!({
        "radius": shape.radius,
        "height": shape.height,
        "noiseFreq": shape.noise_freq,
        "noiseAmp": shape.noise_amp,
        "bevel": shape.bevel,
        "meniscus": shape.meniscus,
    }))
}

fn add_puddle_lobe(
    processes: &mut HashMap<String, GraphNode>,
    connections: &mut Vec<GraphConnection>,
    spec: PuddleLobeSpec,
) {
    let shape_id = format!("{}_shape", spec.name);
    let scale_id = format!("{}_scale", spec.name);

    add_triggered_process(
        processes,
        connections,
        &shape_id,
        "tpl_sdf_puddle",
        puddle_shape_metadata(spec.shape),
    );
    add_process(
        processes,
        &scale_id,
        "tpl_sdf_scale",
        scale_metadata(spec.scale),
    );

    let mut previous = scale_id;
    connections.push(sg_conn(&shape_id, "sdf", &previous, "sdf"));

    if let Some(angle) = spec.rotate_y {
        let rotate_id = format!("{}_rotate", spec.name);
        add_process(
            processes,
            &rotate_id,
            "tpl_sdf_rotate",
            rotate_y_metadata(angle),
        );
        connections.push(sg_conn(&previous, "sdf", &rotate_id, "sdf"));
        previous = rotate_id;
    }

    add_process(
        processes,
        spec.name,
        "tpl_sdf_translate",
        translate_metadata(spec.offset),
    );
    connections.push(sg_conn(&previous, "sdf", spec.name, "sdf"));
}

fn build_ice_material_subgraph() -> Result<SubgraphActor> {
    let graph = GraphExport {
        processes: HashMap::from([
            sg_node("start", "tpl_passthrough", None),
            sg_node(
                "base_noise_scale",
                "tpl_shader_const_float",
                config(json!({ "value": 0.48 })),
            ),
            sg_node(
                "detail_noise_scale",
                "tpl_shader_const_float",
                config(json!({ "value": 2.2 })),
            ),
            sg_node("base_noise", "tpl_shader_noise_texture", None),
            sg_node("detail_noise", "tpl_shader_noise_texture", None),
            sg_node("normal_in", "tpl_shader_normal", None),
            sg_node("position_in", "tpl_shader_position", None),
            sg_node("base_noise_sep", "tpl_shader_separate_xyz", None),
            sg_node("detail_noise_sep", "tpl_shader_separate_xyz", None),
            sg_node("normal_sep", "tpl_shader_separate_xyz", None),
            sg_node("position_sep", "tpl_shader_separate_xyz", None),
            sg_node(
                "normal_abs_y",
                "tpl_shader_math",
                config(json!({ "op": "abs" })),
            ),
            sg_node(
                "pos_abs_x",
                "tpl_shader_math",
                config(json!({ "op": "abs" })),
            ),
            sg_node(
                "pos_abs_y",
                "tpl_shader_math",
                config(json!({ "op": "abs" })),
            ),
            sg_node(
                "pos_abs_z",
                "tpl_shader_math",
                config(json!({ "op": "abs" })),
            ),
            sg_node(
                "pos_sum_xy",
                "tpl_shader_math",
                config(json!({ "op": "add" })),
            ),
            sg_node(
                "pos_sum_xyz",
                "tpl_shader_math",
                config(json!({ "op": "add" })),
            ),
            sg_node(
                "base_noise_soften",
                "tpl_shader_map_range",
                config(json!({
                    "fromMin": 0.0, "fromMax": 1.0, "toMin": 0.24, "toMax": 0.74,
                })),
            ),
            sg_node(
                "ice_density",
                "tpl_shader_map_range",
                config(json!({
                    "fromMin": 0.0, "fromMax": 1.0, "toMin": 0.004, "toMax": 0.035,
                })),
            ),
            sg_node(
                "base_color_mix",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "a": { "type": "constVec3", "c": [0.90, 0.96, 1.0] },
                    "b": { "type": "constVec3", "c": [0.80, 0.90, 0.97] },
                })),
            ),
            sg_node(
                "roughness_mix",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "a": { "type": "constFloat", "c": 0.005 },
                    "b": { "type": "constFloat", "c": 0.015 },
                })),
            ),
            sg_node(
                "transmission_mix",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "a": { "type": "constFloat", "c": 0.96 },
                    "b": { "type": "constFloat", "c": 0.92 },
                })),
            ),
            sg_node(
                "alpha_mix",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "a": { "type": "constFloat", "c": 0.04 },
                    "b": { "type": "constFloat", "c": 0.08 },
                })),
            ),
            sg_node(
                "edge_fresnel",
                "tpl_shader_fresnel",
                config(json!({ "ior": 1.58 })),
            ),
            sg_node(
                "edge_mask",
                "tpl_shader_map_range",
                config(json!({
                    "fromMin": 0.78, "fromMax": 1.10, "toMin": 0.0, "toMax": 1.0,
                })),
            ),
            sg_node(
                "side_mask",
                "tpl_shader_map_range",
                config(json!({
                    "fromMin": 0.0, "fromMax": 0.18, "toMin": 1.0, "toMax": 0.0,
                })),
            ),
            sg_node(
                "side_mask_clamp",
                "tpl_shader_clamp",
                config(json!({ "min": 0.0, "max": 1.0 })),
            ),
            sg_node(
                "shell_mask",
                "tpl_shader_math",
                config(json!({ "op": "multiply" })),
            ),
            sg_node(
                "surface_color",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "b": { "type": "constVec3", "c": [0.86, 0.93, 0.99] },
                })),
            ),
            sg_node(
                "edge_roughness",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "b": { "type": "constFloat", "c": 0.30 },
                })),
            ),
            sg_node(
                "edge_transmission",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "b": { "type": "constFloat", "c": 0.88 },
                })),
            ),
            sg_node(
                "edge_alpha",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "b": { "type": "constFloat", "c": 0.12 },
                })),
            ),
            sg_node(
                "side_color",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "b": { "type": "constVec3", "c": [0.60, 0.72, 0.84] },
                })),
            ),
            sg_node(
                "side_roughness",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "b": { "type": "constFloat", "c": 0.10 },
                })),
            ),
            sg_node(
                "side_transmission",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "b": { "type": "constFloat", "c": 0.92 },
                })),
            ),
            sg_node(
                "side_alpha",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "b": { "type": "constFloat", "c": 0.08 },
                })),
            ),
            sg_node(
                "ior_core",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "a": { "type": "constFloat", "c": 1.305 },
                    "b": { "type": "constFloat", "c": 1.355 },
                })),
            ),
            sg_node(
                "edge_ior",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "b": { "type": "constFloat", "c": 1.375 },
                })),
            ),
            sg_node(
                "side_ior",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                    "b": { "type": "constFloat", "c": 1.37 },
                })),
            ),
            sg_node(
                "detail_height",
                "tpl_shader_map_range",
                config(json!({
                    "fromMin": 0.0, "fromMax": 1.0, "toMin": 0.0, "toMax": 0.005,
                })),
            ),
            sg_node(
                "surface_bump",
                "tpl_shader_bump_map",
                config(json!({
                    "strength": { "type": "constFloat", "c": 0.06 },
                })),
            ),
            sg_node(
                "bsdf",
                "tpl_shader_principled_bsdf",
                config(json!({
                    "metallic": { "type": "constFloat", "c": 0.0 },
                    "emission": { "type": "constVec3", "c": [0.0, 0.0, 0.0] },
                    "emission_strength": { "type": "constFloat", "c": 0.0 },
                    "ior": { "type": "constFloat", "c": 1.34 },
                })),
            ),
            sg_node("mat_out", "tpl_shader_material_output", None),
            sg_node(
                "compiler",
                "tpl_shader_compiler",
                config(json!({ "shadeName": "shade_ice" })),
            ),
            sg_node(
                "shade_slot",
                "tpl_sdf_shade_slot",
                config(json!({ "slot": "ice", "functionName": "shade_ice" })),
            ),
        ]),
        connections: vec![
            sg_conn("start", "output", "base_noise_scale", "_trigger"),
            sg_conn("start", "output", "detail_noise_scale", "_trigger"),
            sg_conn("start", "output", "edge_fresnel", "ior"),
            sg_conn("start", "output", "normal_in", "_trigger"),
            sg_conn("start", "output", "position_in", "_trigger"),
            sg_conn("base_noise_scale", "shader", "base_noise", "scale"),
            sg_conn("detail_noise_scale", "shader", "detail_noise", "scale"),
            sg_conn("normal_in", "shader", "normal_sep", "input"),
            sg_conn("position_in", "shader", "position_sep", "input"),
            sg_conn("normal_sep", "y", "normal_abs_y", "a"),
            sg_conn("position_sep", "x", "pos_abs_x", "a"),
            sg_conn("position_sep", "y", "pos_abs_y", "a"),
            sg_conn("position_sep", "z", "pos_abs_z", "a"),
            sg_conn("pos_abs_x", "shader", "pos_sum_xy", "a"),
            sg_conn("pos_abs_y", "shader", "pos_sum_xy", "b"),
            sg_conn("pos_sum_xy", "shader", "pos_sum_xyz", "a"),
            sg_conn("pos_abs_z", "shader", "pos_sum_xyz", "b"),
            sg_conn("base_noise", "shader", "base_noise_sep", "input"),
            sg_conn("detail_noise", "shader", "detail_noise_sep", "input"),
            sg_conn("base_noise_sep", "x", "base_noise_soften", "input"),
            sg_conn("base_noise_sep", "x", "ice_density", "input"),
            sg_conn("pos_sum_xyz", "shader", "edge_mask", "input"),
            sg_conn("normal_abs_y", "shader", "side_mask", "input"),
            sg_conn("side_mask", "shader", "side_mask_clamp", "input"),
            sg_conn("side_mask_clamp", "shader", "shell_mask", "a"),
            sg_conn("edge_mask", "shader", "shell_mask", "b"),
            sg_conn("base_noise_soften", "shader", "base_color_mix", "fac"),
            sg_conn("base_color_mix", "shader", "surface_color", "a"),
            sg_conn("edge_mask", "shader", "surface_color", "fac"),
            sg_conn("ice_density", "shader", "roughness_mix", "fac"),
            sg_conn("ice_density", "shader", "transmission_mix", "fac"),
            sg_conn("ice_density", "shader", "alpha_mix", "fac"),
            sg_conn("roughness_mix", "shader", "edge_roughness", "a"),
            sg_conn("edge_mask", "shader", "edge_roughness", "fac"),
            sg_conn("transmission_mix", "shader", "edge_transmission", "a"),
            sg_conn("edge_mask", "shader", "edge_transmission", "fac"),
            sg_conn("alpha_mix", "shader", "edge_alpha", "a"),
            sg_conn("edge_mask", "shader", "edge_alpha", "fac"),
            sg_conn("surface_color", "shader", "side_color", "a"),
            sg_conn("shell_mask", "shader", "side_color", "fac"),
            sg_conn("edge_roughness", "shader", "side_roughness", "a"),
            sg_conn("shell_mask", "shader", "side_roughness", "fac"),
            sg_conn("edge_transmission", "shader", "side_transmission", "a"),
            sg_conn("shell_mask", "shader", "side_transmission", "fac"),
            sg_conn("edge_alpha", "shader", "side_alpha", "a"),
            sg_conn("shell_mask", "shader", "side_alpha", "fac"),
            sg_conn("ice_density", "shader", "ior_core", "fac"),
            sg_conn("ior_core", "shader", "edge_ior", "a"),
            sg_conn("edge_mask", "shader", "edge_ior", "fac"),
            sg_conn("edge_ior", "shader", "side_ior", "a"),
            sg_conn("shell_mask", "shader", "side_ior", "fac"),
            sg_conn("detail_noise_sep", "x", "detail_height", "input"),
            sg_conn("detail_height", "shader", "surface_bump", "height"),
            sg_conn("side_color", "shader", "bsdf", "base_color"),
            sg_conn("side_roughness", "shader", "bsdf", "roughness"),
            sg_conn("side_transmission", "shader", "bsdf", "transmission"),
            sg_conn("side_alpha", "shader", "bsdf", "alpha"),
            sg_conn("side_ior", "shader", "bsdf", "ior"),
            sg_conn("surface_bump", "shader", "bsdf", "normal"),
            sg_conn("bsdf", "shader", "mat_out", "surface"),
            sg_conn("mat_out", "shader", "compiler", "shader"),
            sg_conn("compiler", "shade", "shade_slot", "shade"),
        ],
        inports: HashMap::from([("trigger".to_string(), sg_edge("start", "input"))]),
        outports: HashMap::from([("shade".to_string(), sg_edge("shade_slot", "shade"))]),
        ..Default::default()
    };

    SubgraphActor::from_graph_export(
        &graph,
        template_actors(&[
            "tpl_passthrough",
            "tpl_shader_const_float",
            "tpl_shader_noise_texture",
            "tpl_shader_normal",
            "tpl_shader_position",
            "tpl_shader_math",
            "tpl_shader_separate_xyz",
            "tpl_shader_clamp",
            "tpl_shader_map_range",
            "tpl_shader_color_mix",
            "tpl_shader_fresnel",
            "tpl_shader_bump_map",
            "tpl_shader_principled_bsdf",
            "tpl_shader_material_output",
            "tpl_shader_compiler",
            "tpl_sdf_shade_slot",
        ])?,
    )
}

fn build_water_material_subgraph() -> Result<SubgraphActor> {
    let graph = GraphExport {
        processes: HashMap::from([
            sg_node("start", "tpl_passthrough", None),
            sg_node(
                "ripple_scale",
                "tpl_shader_const_float",
                config(json!({ "value": 0.82 })),
            ),
            sg_node("ripple_noise", "tpl_shader_noise_texture", None),
            sg_node("ripple_sep", "tpl_shader_separate_xyz", None),
            sg_node(
                "ripple_height",
                "tpl_shader_map_range",
                config(json!({
                    "fromMin": 0.0, "fromMax": 1.0, "toMin": 0.0, "toMax": 0.0032,
                })),
            ),
            sg_node(
                "surface_bump",
                "tpl_shader_bump_map",
                config(json!({
                    "strength": { "type": "constFloat", "c": 0.0038 },
                })),
            ),
            sg_node(
                "bsdf",
                "tpl_shader_principled_bsdf",
                config(json!({
                    "base_color": { "type": "constVec3", "c": [0.92, 0.97, 1.0] },
                    "metallic": { "type": "constFloat", "c": 0.0 },
                    "roughness": { "type": "constFloat", "c": 0.005 },
                    "emission": { "type": "constVec3", "c": [0.0, 0.0, 0.0] },
                    "emission_strength": { "type": "constFloat", "c": 0.0 },
                    "alpha": { "type": "constFloat", "c": 0.014 },
                    "transmission": { "type": "constFloat", "c": 0.982 },
                    "ior": { "type": "constFloat", "c": 1.337 },
                })),
            ),
            sg_node("mat_out", "tpl_shader_material_output", None),
            sg_node(
                "compiler",
                "tpl_shader_compiler",
                config(json!({ "shadeName": "shade_water" })),
            ),
            sg_node(
                "shade_slot",
                "tpl_sdf_shade_slot",
                config(json!({ "slot": "water", "functionName": "shade_water" })),
            ),
        ]),
        connections: vec![
            sg_conn("start", "output", "ripple_scale", "_trigger"),
            sg_conn("ripple_scale", "shader", "ripple_noise", "scale"),
            sg_conn("ripple_noise", "shader", "ripple_sep", "input"),
            sg_conn("ripple_sep", "x", "ripple_height", "input"),
            sg_conn("ripple_height", "shader", "surface_bump", "height"),
            sg_conn("surface_bump", "shader", "bsdf", "normal"),
            sg_conn("bsdf", "shader", "mat_out", "surface"),
            sg_conn("mat_out", "shader", "compiler", "shader"),
            sg_conn("compiler", "shade", "shade_slot", "shade"),
        ],
        inports: HashMap::from([("trigger".to_string(), sg_edge("start", "input"))]),
        outports: HashMap::from([("shade".to_string(), sg_edge("shade_slot", "shade"))]),
        ..Default::default()
    };

    SubgraphActor::from_graph_export(
        &graph,
        template_actors(&[
            "tpl_passthrough",
            "tpl_shader_const_float",
            "tpl_shader_noise_texture",
            "tpl_shader_separate_xyz",
            "tpl_shader_map_range",
            "tpl_shader_bump_map",
            "tpl_shader_principled_bsdf",
            "tpl_shader_material_output",
            "tpl_shader_compiler",
            "tpl_sdf_shade_slot",
        ])?,
    )
}

fn build_floor_material_subgraph() -> Result<SubgraphActor> {
    let graph = GraphExport {
        processes: HashMap::from([
            sg_node("start", "tpl_passthrough", None),
            sg_node(
                "floor_near_color",
                "tpl_shader_const_color",
                config(json!({ "r": 0.92, "g": 0.95, "b": 0.98 })),
            ),
            sg_node(
                "floor_far_color",
                "tpl_shader_const_color",
                config(json!({ "r": 0.42, "g": 0.52, "b": 0.64 })),
            ),
            sg_node("floor_position", "tpl_shader_position", None),
            sg_node("floor_pos_z", "tpl_shader_separate_xyz", None),
            sg_node(
                "floor_gradient_fac",
                "tpl_shader_map_range",
                config(json!({
                    "fromMin": -2.2, "fromMax": 7.4, "toMin": 1.0, "toMax": 0.0,
                })),
            ),
            sg_node(
                "floor_surface_color",
                "tpl_shader_color_mix",
                config(json!({
                    "mode": "mix",
                })),
            ),
            sg_node(
                "floor_metallic",
                "tpl_shader_const_float",
                config(json!({ "value": 0.0 })),
            ),
            sg_node(
                "floor_roughness",
                "tpl_shader_const_float",
                config(json!({ "value": 0.012 })),
            ),
            sg_node(
                "floor_emission_strength",
                "tpl_shader_const_float",
                config(json!({ "value": 0.55 })),
            ),
            sg_node(
                "floor_alpha",
                "tpl_shader_const_float",
                config(json!({ "value": 1.0 })),
            ),
            sg_node(
                "floor_transmission",
                "tpl_shader_const_float",
                config(json!({ "value": 0.0 })),
            ),
            sg_node(
                "floor_ior",
                "tpl_shader_const_float",
                config(json!({ "value": 1.2 })),
            ),
            sg_node(
                "bsdf",
                "tpl_shader_principled_bsdf",
                None,
            ),
            sg_node("mat_out", "tpl_shader_material_output", None),
            sg_node(
                "compiler",
                "tpl_shader_compiler",
                config(json!({ "shadeName": "shade_floor" })),
            ),
            sg_node(
                "shade_slot",
                "tpl_sdf_shade_slot",
                config(json!({ "slot": "floor", "functionName": "shade_floor" })),
            ),
        ]),
        connections: vec![
            sg_conn("start", "output", "floor_near_color", "_trigger"),
            sg_conn("start", "output", "floor_far_color", "_trigger"),
            sg_conn("start", "output", "floor_position", "_trigger"),
            sg_conn("start", "output", "floor_metallic", "_trigger"),
            sg_conn("start", "output", "floor_roughness", "_trigger"),
            sg_conn("start", "output", "floor_emission_strength", "_trigger"),
            sg_conn("start", "output", "floor_alpha", "_trigger"),
            sg_conn("start", "output", "floor_transmission", "_trigger"),
            sg_conn("start", "output", "floor_ior", "_trigger"),
            sg_conn("floor_position", "shader", "floor_pos_z", "input"),
            sg_conn("floor_pos_z", "z", "floor_gradient_fac", "input"),
            sg_conn("floor_gradient_fac", "shader", "floor_surface_color", "fac"),
            sg_conn("floor_near_color", "shader", "floor_surface_color", "a"),
            sg_conn("floor_far_color", "shader", "floor_surface_color", "b"),
            sg_conn("floor_surface_color", "shader", "bsdf", "base_color"),
            sg_conn("floor_metallic", "shader", "bsdf", "metallic"),
            sg_conn("floor_roughness", "shader", "bsdf", "roughness"),
            sg_conn("floor_surface_color", "shader", "bsdf", "emission"),
            sg_conn("floor_emission_strength", "shader", "bsdf", "emission_strength"),
            sg_conn("floor_alpha", "shader", "bsdf", "alpha"),
            sg_conn("floor_transmission", "shader", "bsdf", "transmission"),
            sg_conn("floor_ior", "shader", "bsdf", "ior"),
            sg_conn("bsdf", "shader", "mat_out", "surface"),
            sg_conn("mat_out", "shader", "compiler", "shader"),
            sg_conn("compiler", "shade", "shade_slot", "shade"),
        ],
        inports: HashMap::from([("trigger".to_string(), sg_edge("start", "input"))]),
        outports: HashMap::from([("shade".to_string(), sg_edge("shade_slot", "shade"))]),
        ..Default::default()
    };

    SubgraphActor::from_graph_export(
        &graph,
        template_actors(&[
            "tpl_passthrough",
            "tpl_shader_const_color",
            "tpl_shader_const_float",
            "tpl_shader_position",
            "tpl_shader_separate_xyz",
            "tpl_shader_map_range",
            "tpl_shader_color_mix",
            "tpl_shader_principled_bsdf",
            "tpl_shader_material_output",
            "tpl_shader_compiler",
            "tpl_sdf_shade_slot",
        ])?,
    )
}

fn build_backdrop_material_subgraph() -> Result<SubgraphActor> {
    let graph = GraphExport {
        processes: HashMap::from([
            sg_node("start", "tpl_passthrough", None),
            sg_node(
                "card_low_color",
                "tpl_shader_const_color",
                config(json!({ "r": 0.88, "g": 0.93, "b": 0.98 })),
            ),
            sg_node(
                "card_high_color",
                "tpl_shader_const_color",
                config(json!({ "r": 0.96, "g": 0.98, "b": 1.0 })),
            ),
            sg_node(
                "card_emission_low",
                "tpl_shader_const_color",
                config(json!({ "r": 0.82, "g": 0.88, "b": 0.96 })),
            ),
            sg_node(
                "card_emission_high",
                "tpl_shader_const_color",
                config(json!({ "r": 0.92, "g": 0.96, "b": 1.0 })),
            ),
            sg_node(
                "card_position",
                "tpl_shader_position",
                None,
            ),
            sg_node("card_pos_sep", "tpl_shader_separate_xyz", None),
            sg_node(
                "card_height_fac",
                "tpl_shader_map_range",
                config(json!({
                    "fromMin": -1.1, "fromMax": 2.6, "toMin": 0.0, "toMax": 1.0,
                })),
            ),
            sg_node(
                "card_surface_color",
                "tpl_shader_color_mix",
                config(json!({ "mode": "mix" })),
            ),
            sg_node(
                "card_emission_color",
                "tpl_shader_color_mix",
                config(json!({ "mode": "mix" })),
            ),
            sg_node(
                "card_metallic",
                "tpl_shader_const_float",
                config(json!({ "value": 0.0 })),
            ),
            sg_node(
                "card_roughness",
                "tpl_shader_const_float",
                config(json!({ "value": 0.16 })),
            ),
            sg_node(
                "card_emission_strength",
                "tpl_shader_const_float",
                config(json!({ "value": 1.2 })),
            ),
            sg_node(
                "card_alpha",
                "tpl_shader_const_float",
                config(json!({ "value": 1.0 })),
            ),
            sg_node(
                "card_transmission",
                "tpl_shader_const_float",
                config(json!({ "value": 0.0 })),
            ),
            sg_node(
                "card_ior",
                "tpl_shader_const_float",
                config(json!({ "value": 1.2 })),
            ),
            sg_node("bsdf", "tpl_shader_principled_bsdf", None),
            sg_node("mat_out", "tpl_shader_material_output", None),
            sg_node(
                "compiler",
                "tpl_shader_compiler",
                config(json!({ "shadeName": "shade_backdrop" })),
            ),
            sg_node(
                "shade_slot",
                "tpl_sdf_shade_slot",
                config(json!({ "slot": "backdrop", "functionName": "shade_backdrop" })),
            ),
        ]),
        connections: vec![
            sg_conn("start", "output", "card_low_color", "_trigger"),
            sg_conn("start", "output", "card_high_color", "_trigger"),
            sg_conn("start", "output", "card_emission_low", "_trigger"),
            sg_conn("start", "output", "card_emission_high", "_trigger"),
            sg_conn("start", "output", "card_position", "_trigger"),
            sg_conn("start", "output", "card_metallic", "_trigger"),
            sg_conn("start", "output", "card_roughness", "_trigger"),
            sg_conn("start", "output", "card_emission_strength", "_trigger"),
            sg_conn("start", "output", "card_alpha", "_trigger"),
            sg_conn("start", "output", "card_transmission", "_trigger"),
            sg_conn("start", "output", "card_ior", "_trigger"),
            sg_conn("card_position", "shader", "card_pos_sep", "input"),
            sg_conn("card_pos_sep", "y", "card_height_fac", "input"),
            sg_conn("card_height_fac", "shader", "card_surface_color", "fac"),
            sg_conn("card_low_color", "shader", "card_surface_color", "a"),
            sg_conn("card_high_color", "shader", "card_surface_color", "b"),
            sg_conn("card_height_fac", "shader", "card_emission_color", "fac"),
            sg_conn("card_emission_low", "shader", "card_emission_color", "a"),
            sg_conn("card_emission_high", "shader", "card_emission_color", "b"),
            sg_conn("card_surface_color", "shader", "bsdf", "base_color"),
            sg_conn("card_metallic", "shader", "bsdf", "metallic"),
            sg_conn("card_roughness", "shader", "bsdf", "roughness"),
            sg_conn("card_emission_color", "shader", "bsdf", "emission"),
            sg_conn("card_emission_strength", "shader", "bsdf", "emission_strength"),
            sg_conn("card_alpha", "shader", "bsdf", "alpha"),
            sg_conn("card_transmission", "shader", "bsdf", "transmission"),
            sg_conn("card_ior", "shader", "bsdf", "ior"),
            sg_conn("bsdf", "shader", "mat_out", "surface"),
            sg_conn("mat_out", "shader", "compiler", "shader"),
            sg_conn("compiler", "shade", "shade_slot", "shade"),
        ],
        inports: HashMap::from([("trigger".to_string(), sg_edge("start", "input"))]),
        outports: HashMap::from([("shade".to_string(), sg_edge("shade_slot", "shade"))]),
        ..Default::default()
    };

    SubgraphActor::from_graph_export(
        &graph,
        template_actors(&[
            "tpl_passthrough",
            "tpl_shader_const_color",
            "tpl_shader_const_float",
            "tpl_shader_position",
            "tpl_shader_separate_xyz",
            "tpl_shader_map_range",
            "tpl_shader_color_mix",
            "tpl_shader_principled_bsdf",
            "tpl_shader_material_output",
            "tpl_shader_compiler",
            "tpl_sdf_shade_slot",
        ])?,
    )
}

fn build_ice_geometry_subgraph() -> Result<SubgraphActor> {
    let mut processes = HashMap::new();
    let mut connections = Vec::new();

    add_process(&mut processes, "start", "tpl_passthrough", None);
    add_triggered_process(
        &mut processes,
        &mut connections,
        "ice_outer_shape",
        "tpl_sdf_round_box",
        config(json!({
            "sizeX": 0.47, "sizeY": 0.47, "sizeZ": 0.47,
            "radius": 0.028,
        })),
    );
    add_process(
        &mut processes,
        "ice_displace_broad",
        "tpl_sdf_displace",
        config(json!({
            "frequency": 2.2, "amplitude": 0.025, "octaves": 3,
        })),
    );
    add_process(
        &mut processes,
        "ice_stamp_compose",
        "tpl_sdf_stamp_compose",
        config(json!({
            "stamps": [
                {
                    "shape": "ellipsoid",
                    "radii": [0.384, 0.090, 0.384],
                    "offset": [0.0, -0.34, 0.0],
                    "op": "smoothUnion",
                    "smoothness": 0.09
                },
                {
                    "shape": "ellipsoid",
                    "radii": [0.238, 0.101, 0.123],
                    "offset": [0.18, 0.24, 0.19],
                    "op": "smoothUnion",
                    "smoothness": 0.05
                },
                {
                    "shape": "ellipsoid",
                    "radii": [0.206, 0.058, 0.113],
                    "offset": [-0.04, -0.18, 0.20],
                    "op": "smoothUnion",
                    "smoothness": 0.055
                },
                {
                    "shape": "roundBox",
                    "size": [0.040, 0.056, 0.040],
                    "radius": 0.020,
                    "offset": [0.39, 0.29, -0.38],
                    "angles": [-9.0, 15.0, 6.0],
                    "op": "smoothDifference",
                    "smoothness": 0.046
                },
                {
                    "shape": "roundBox",
                    "size": [0.068, 0.010, 0.050],
                    "radius": 0.016,
                    "offset": [0.13, 0.456, 0.15],
                    "angles": [-6.0, 11.0, 3.0],
                    "op": "smoothDifference",
                    "smoothness": 0.048
                },
                {
                    "shape": "roundBox",
                    "size": [0.034, 0.044, 0.034],
                    "radius": 0.016,
                    "offset": [-0.37, 0.31, 0.34],
                    "angles": [8.0, -18.0, 7.0],
                    "op": "smoothDifference",
                    "smoothness": 0.042
                }
            ]
        })),
    );

    add_process(
        &mut processes,
        "ice_displace_fine",
        "tpl_sdf_displace",
        config(json!({
            "frequency": 2.10, "amplitude": 0.013, "octaves": 2,
        })),
    );
    add_process(
        &mut processes,
        "ice_tilt",
        "tpl_sdf_rotate",
        config(json!({ "x": -2.5, "y": 6.0, "z": 1.2 })),
    );
    add_process(
        &mut processes,
        "ice_settle",
        "tpl_sdf_translate",
        config(json!({ "x": 0.0, "y": -0.008, "z": 0.0 })),
    );
    add_process(
        &mut processes,
        "ice_material",
        "tpl_sdf_material",
        config(json!({ "slot": "ice" })),
    );

    connections.extend([
        sg_conn("ice_outer_shape", "sdf", "ice_displace_broad", "sdf"),
        sg_conn("ice_displace_broad", "sdf", "ice_stamp_compose", "sdf"),
        sg_conn("ice_stamp_compose", "sdf", "ice_displace_fine", "sdf"),
        sg_conn("ice_displace_fine", "sdf", "ice_tilt", "sdf"),
        sg_conn("ice_tilt", "sdf", "ice_settle", "sdf"),
        sg_conn("ice_settle", "sdf", "ice_material", "sdf"),
    ]);

    let graph = GraphExport {
        processes,
        connections,
        inports: HashMap::from([("trigger".to_string(), sg_edge("start", "input"))]),
        outports: HashMap::from([("sdf".to_string(), sg_edge("ice_material", "sdf"))]),
        ..Default::default()
    };

    SubgraphActor::from_graph_export(
        &graph,
        template_actors(&[
            "tpl_passthrough",
            "tpl_sdf_round_box",
            "tpl_sdf_displace",
            "tpl_sdf_rotate",
            "tpl_sdf_stamp_compose",
            "tpl_sdf_translate",
            "tpl_sdf_material",
        ])?,
    )
}

fn build_backdrop_geometry_subgraph() -> Result<SubgraphActor> {
    let graph = GraphExport {
        processes: HashMap::from([
            sg_node("start", "tpl_passthrough", None),
            sg_node(
                "back_card_shape",
                "tpl_sdf_box",
                config(json!({
                    "sizeX": 3.8, "sizeY": 2.8, "sizeZ": 0.04,
                })),
            ),
            sg_node(
                "back_card",
                "tpl_sdf_translate",
                config(json!({ "x": 0.0, "y": 0.96, "z": -4.30 })),
            ),
            sg_node(
                "left_card_shape",
                "tpl_sdf_box",
                config(json!({
                    "sizeX": 0.04, "sizeY": 2.8, "sizeZ": 3.6,
                })),
            ),
            sg_node(
                "left_card_rotate",
                "tpl_sdf_rotate",
                config(json!({ "y": 32.0 })),
            ),
            sg_node(
                "left_card",
                "tpl_sdf_translate",
                config(json!({ "x": -4.40, "y": 1.00, "z": -0.80 })),
            ),
            sg_node(
                "right_card_shape",
                "tpl_sdf_box",
                config(json!({
                    "sizeX": 0.04, "sizeY": 2.8, "sizeZ": 3.6,
                })),
            ),
            sg_node(
                "right_card_rotate",
                "tpl_sdf_rotate",
                config(json!({ "y": -32.0 })),
            ),
            sg_node(
                "right_card",
                "tpl_sdf_translate",
                config(json!({ "x": 4.40, "y": 1.00, "z": -0.80 })),
            ),
            sg_node(
                "top_card_shape",
                "tpl_sdf_box",
                config(json!({
                    "sizeX": 3.6, "sizeY": 0.04, "sizeZ": 3.4,
                })),
            ),
            sg_node(
                "top_card",
                "tpl_sdf_translate",
                config(json!({ "x": 0.0, "y": 3.55, "z": -0.35 })),
            ),
            sg_node(
                "front_left_shape",
                "tpl_sdf_box",
                config(json!({
                    "sizeX": 0.85, "sizeY": 1.20, "sizeZ": 0.04,
                })),
            ),
            sg_node(
                "front_left_rotate",
                "tpl_sdf_rotate",
                config(json!({ "y": 58.0 })),
            ),
            sg_node(
                "front_left",
                "tpl_sdf_translate",
                config(json!({ "x": -4.90, "y": 1.25, "z": 1.65 })),
            ),
            sg_node(
                "front_right_shape",
                "tpl_sdf_box",
                config(json!({
                    "sizeX": 0.85, "sizeY": 1.20, "sizeZ": 0.04,
                })),
            ),
            sg_node(
                "front_right_rotate",
                "tpl_sdf_rotate",
                config(json!({ "y": -58.0 })),
            ),
            sg_node(
                "front_right",
                "tpl_sdf_translate",
                config(json!({ "x": 4.90, "y": 1.25, "z": 1.65 })),
            ),
            sg_node("card_union_a", "tpl_sdf_union", None),
            sg_node("card_union_b", "tpl_sdf_union", None),
            sg_node("card_union_c", "tpl_sdf_union", None),
            sg_node("card_union_d", "tpl_sdf_union", None),
            sg_node("card_union_e", "tpl_sdf_union", None),
            sg_node(
                "backdrop_material",
                "tpl_sdf_material",
                config(json!({ "slot": "backdrop" })),
            ),
        ]),
        connections: vec![
            sg_conn("start", "output", "back_card_shape", "_trigger"),
            sg_conn("start", "output", "left_card_shape", "_trigger"),
            sg_conn("start", "output", "right_card_shape", "_trigger"),
            sg_conn("start", "output", "top_card_shape", "_trigger"),
            sg_conn("start", "output", "front_left_shape", "_trigger"),
            sg_conn("start", "output", "front_right_shape", "_trigger"),
            sg_conn("back_card_shape", "sdf", "back_card", "sdf"),
            sg_conn("left_card_shape", "sdf", "left_card_rotate", "sdf"),
            sg_conn("left_card_rotate", "sdf", "left_card", "sdf"),
            sg_conn("right_card_shape", "sdf", "right_card_rotate", "sdf"),
            sg_conn("right_card_rotate", "sdf", "right_card", "sdf"),
            sg_conn("top_card_shape", "sdf", "top_card", "sdf"),
            sg_conn("front_left_shape", "sdf", "front_left_rotate", "sdf"),
            sg_conn("front_left_rotate", "sdf", "front_left", "sdf"),
            sg_conn("front_right_shape", "sdf", "front_right_rotate", "sdf"),
            sg_conn("front_right_rotate", "sdf", "front_right", "sdf"),
            sg_conn("back_card", "sdf", "card_union_a", "sdf_a"),
            sg_conn("left_card", "sdf", "card_union_a", "sdf_b"),
            sg_conn("card_union_a", "sdf", "card_union_b", "sdf_a"),
            sg_conn("right_card", "sdf", "card_union_b", "sdf_b"),
            sg_conn("card_union_b", "sdf", "card_union_c", "sdf_a"),
            sg_conn("top_card", "sdf", "card_union_c", "sdf_b"),
            sg_conn("card_union_c", "sdf", "card_union_d", "sdf_a"),
            sg_conn("front_left", "sdf", "card_union_d", "sdf_b"),
            sg_conn("card_union_d", "sdf", "card_union_e", "sdf_a"),
            sg_conn("front_right", "sdf", "card_union_e", "sdf_b"),
            sg_conn("card_union_e", "sdf", "backdrop_material", "sdf"),
        ],
        inports: HashMap::from([("trigger".to_string(), sg_edge("start", "input"))]),
        outports: HashMap::from([("sdf".to_string(), sg_edge("backdrop_material", "sdf"))]),
        ..Default::default()
    };

    SubgraphActor::from_graph_export(
        &graph,
        template_actors(&[
            "tpl_passthrough",
            "tpl_sdf_box",
            "tpl_sdf_translate",
            "tpl_sdf_rotate",
            "tpl_sdf_union",
            "tpl_sdf_material",
        ])?,
    )
}

fn build_puddle_geometry_subgraph() -> Result<SubgraphActor> {
    let mut processes = HashMap::new();
    let mut connections = Vec::new();

    add_process(&mut processes, "start", "tpl_passthrough", None);

    for spec in [
        PuddleLobeSpec {
            name: "puddle_core",
            shape: PuddleShapeSpec {
                radius: 0.98,
                height: 0.026,
                noise_freq: 1.0,
                noise_amp: 0.18,
                bevel: 0.42,
                meniscus: 0.009,
            },
            scale: [1.34, 1.42, 0.98],
            rotate_y: None,
            offset: [0.05, -0.55, 0.03],
        },
        PuddleLobeSpec {
            name: "puddle_tail",
            shape: PuddleShapeSpec {
                radius: 0.50,
                height: 0.0165,
                noise_freq: 1.35,
                noise_amp: 0.12,
                bevel: 0.28,
                meniscus: 0.0052,
            },
            scale: [2.2, 1.22, 0.52],
            rotate_y: Some(22.0),
            offset: [0.98, -0.55, -0.08],
        },
        PuddleLobeSpec {
            name: "puddle_side",
            shape: PuddleShapeSpec {
                radius: 0.42,
                height: 0.0145,
                noise_freq: 1.25,
                noise_amp: 0.10,
                bevel: 0.24,
                meniscus: 0.0044,
            },
            scale: [1.62, 1.18, 0.66],
            rotate_y: Some(-38.0),
            offset: [-0.82, -0.55, 0.34],
        },
        PuddleLobeSpec {
            name: "puddle_tip",
            shape: PuddleShapeSpec {
                radius: 0.27,
                height: 0.012,
                noise_freq: 1.55,
                noise_amp: 0.08,
                bevel: 0.18,
                meniscus: 0.0034,
            },
            scale: [1.68, 1.14, 0.58],
            rotate_y: Some(-8.0),
            offset: [-0.12, -0.55, -0.94],
        },
        PuddleLobeSpec {
            name: "droplet_a",
            shape: PuddleShapeSpec {
                radius: 0.13,
                height: 0.0115,
                noise_freq: 1.2,
                noise_amp: 0.05,
                bevel: 0.11,
                meniscus: 0.0024,
            },
            scale: [1.18, 1.0, 0.96],
            rotate_y: None,
            offset: [1.54, -0.55, 0.72],
        },
        PuddleLobeSpec {
            name: "droplet_b",
            shape: PuddleShapeSpec {
                radius: 0.10,
                height: 0.009,
                noise_freq: 1.4,
                noise_amp: 0.04,
                bevel: 0.085,
                meniscus: 0.0020,
            },
            scale: [1.05, 1.0, 0.88],
            rotate_y: None,
            offset: [-1.28, -0.55, -1.12],
        },
    ] {
        add_puddle_lobe(&mut processes, &mut connections, spec);
    }

    for (id, smoothness, left, right) in [
        ("puddle_union_a", 0.075, "puddle_core", "puddle_tail"),
        ("puddle_union_b", 0.064, "puddle_union_a", "puddle_side"),
        ("puddle_union_c", 0.052, "puddle_union_b", "puddle_tip"),
        ("puddle_union_d", 0.034, "puddle_union_c", "droplet_a"),
        ("puddle_union_e", 0.028, "puddle_union_d", "droplet_b"),
    ] {
        add_sdf_binary_op(
            &mut processes,
            &mut connections,
            id,
            "tpl_sdf_smooth_union",
            config(json!({ "smoothness": smoothness })),
            left,
            right,
        );
    }

    add_process(
        &mut processes,
        "puddle_surface",
        "tpl_sdf_displace",
        config(json!({
            "frequency": 0.52, "amplitude": 0.0007, "octaves": 2,
        })),
    );
    add_process(
        &mut processes,
        "water_material",
        "tpl_sdf_material",
        config(json!({ "slot": "water" })),
    );

    connections.extend([
        sg_conn("puddle_union_e", "sdf", "puddle_surface", "sdf"),
        sg_conn("puddle_surface", "sdf", "water_material", "sdf"),
    ]);

    let graph = GraphExport {
        processes,
        connections,
        inports: HashMap::from([("trigger".to_string(), sg_edge("start", "input"))]),
        outports: HashMap::from([("sdf".to_string(), sg_edge("water_material", "sdf"))]),
        ..Default::default()
    };

    SubgraphActor::from_graph_export(
        &graph,
        template_actors(&[
            "tpl_passthrough",
            "tpl_sdf_puddle",
            "tpl_sdf_scale",
            "tpl_sdf_translate",
            "tpl_sdf_rotate",
            "tpl_sdf_smooth_union",
            "tpl_sdf_displace",
            "tpl_sdf_material",
        ])?,
    )
}

fn build_floor_geometry_subgraph() -> Result<SubgraphActor> {
    let graph = GraphExport {
        processes: HashMap::from([
            sg_node("start", "tpl_passthrough", None),
            sg_node(
                "floor_slab_shape",
                "tpl_sdf_box",
                config(json!({
                    "sizeX": 10.0, "sizeY": 0.06, "sizeZ": 10.0,
                })),
            ),
            sg_node(
                "floor_slab",
                "tpl_sdf_translate",
                config(json!({ "x": 0.0, "y": -0.61, "z": 0.0 })),
            ),
            sg_node(
                "floor_material",
                "tpl_sdf_material",
                config(json!({ "slot": "floor" })),
            ),
        ]),
        connections: vec![
            sg_conn("start", "output", "floor_slab_shape", "_trigger"),
            sg_conn("floor_slab_shape", "sdf", "floor_slab", "sdf"),
            sg_conn("floor_slab", "sdf", "floor_material", "sdf"),
        ],
        inports: HashMap::from([("trigger".to_string(), sg_edge("start", "input"))]),
        outports: HashMap::from([("sdf".to_string(), sg_edge("floor_material", "sdf"))]),
        ..Default::default()
    };

    SubgraphActor::from_graph_export(
        &graph,
        template_actors(&[
            "tpl_passthrough",
            "tpl_sdf_box",
            "tpl_sdf_translate",
            "tpl_sdf_material",
        ])?,
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Melting Ice Cube — SDF + Material Subgraphs ===\n");

    let fps = env_u32("REFLOW_FPS", 24);
    let duration = env_f64("REFLOW_DURATION_SECS", 3.0);
    let total_frames = ((duration * fps as f64).round()).max(1.0) as usize;
    let w = env_u32("REFLOW_WIDTH", 960);
    let h = env_u32("REFLOW_HEIGHT", 960);
    let max_steps = env_u32("REFLOW_MAX_STEPS", 220);

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        "tpl_collect",
        "tpl_sdf_union",
        "tpl_sdf_difference",
        "tpl_sdf_smooth_difference",
        "tpl_sdf_smooth_union",
        "tpl_sdf_scene",
        "tpl_sdf_render",
        "tpl_interval_trigger",
        "tpl_animation_time",
        "tpl_gpu_2d_render",
        "tpl_render_frame_collector",
        "tpl_video_encoder",
        "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    net.register_actor_arc("IceMaterialGraph", Arc::new(build_ice_material_subgraph()?))?;
    net.register_actor_arc("WaterMaterialGraph", Arc::new(build_water_material_subgraph()?))?;
    net.register_actor_arc("IceGeometryGraph", Arc::new(build_ice_geometry_subgraph()?))?;
    net.register_actor_arc(
        "PuddleGeometryGraph",
        Arc::new(build_puddle_geometry_subgraph()?),
    )?;
    net.add_node("ice_material", "IceMaterialGraph", None)?;
    net.add_node("water_material", "WaterMaterialGraph", None)?;
    net.add_node(
        "materials_ready",
        "tpl_collect",
        config(json!({ "count": 2 })),
    )?;
    net.add_node("ice_geometry", "IceGeometryGraph", None)?;
    net.add_node("puddle_geometry", "PuddleGeometryGraph", None)?;
    net.add_node(
        "puddle_contact_cut",
        "tpl_sdf_smooth_difference",
        config(json!({ "smoothness": 0.012 })),
    )?;
    net.add_node("shape_union", "tpl_sdf_union", None)?;
    net.add_node(
        "sdf_scene",
        "tpl_sdf_scene",
        config(json!({
            "softShadows": true,
            "shadowK": 10.0,
            "ao": false,
            "ambient": 0.02,
            "requireShade": true,
            "logProgress": true,
        })),
    )?;
    net.add_node(
        "render",
        "tpl_sdf_render",
        config(json!({
            "width": w, "height": h,
            "maxSteps": max_steps, "fov": 50.0,
            "cameraPosX": 3.8, "cameraPosY": 1.6, "cameraPosZ": 4.2,
            "cameraTargetX": 0.0, "cameraTargetY": -0.15, "cameraTargetZ": 0.0,
            "softShadows": false, "shadowK": 10.0, "ao": false,
            "ambient": 0.015,
            "lightDir": [-0.72, 0.48, -0.50],
            "lightColor": [1.8, 1.85, 1.9],
            "background": [0.38, 0.46, 0.56],
            "shadowK": 16.0,
            "logProgress": true,
        })),
    )?;

    net.add_connection(wire("ice_material", "shade", "sdf_scene", "shade"));
    net.add_connection(wire("water_material", "shade", "sdf_scene", "shade"));
    net.add_connection(wire("ice_material", "shade", "materials_ready", "input"));
    net.add_connection(wire("water_material", "shade", "materials_ready", "input"));
    net.add_connection(wire("materials_ready", "output", "ice_geometry", "trigger"));
    net.add_connection(wire(
        "materials_ready",
        "output",
        "puddle_geometry",
        "trigger",
    ));
    net.add_connection(wire("ice_geometry", "sdf", "shape_union", "sdf_a"));
    net.add_connection(wire(
        "puddle_geometry",
        "sdf",
        "puddle_contact_cut",
        "sdf_a",
    ));
    net.add_connection(wire("ice_geometry", "sdf", "puddle_contact_cut", "sdf_b"));
    net.add_connection(wire(
        "puddle_contact_cut",
        "sdf",
        "shape_union",
        "sdf_b",
    ));
    net.add_connection(wire("shape_union", "sdf", "sdf_scene", "sdf"));
    net.add_connection(wire("sdf_scene", "sdf", "render", "sdf"));

    net.add_node(
        "tick",
        "tpl_interval_trigger",
        config(json!({
            "interval": 1000 / fps as u64,
            "maxExecutions": total_frames,
            "startImmediately": false,
        })),
    )?;
    net.add_node(
        "anim_time",
        "tpl_animation_time",
        config(json!({ "fps": fps, "speed": 1.0, "logProgress": true })),
    )?;
    net.add_connection(wire("tick", "trigger", "anim_time", "trigger"));
    net.add_connection(wire("anim_time", "time", "render", "time"));
    net.add_connection(wire("sdf_scene", "stats", "tick", "start"));

    net.add_node(
        "composite",
        "tpl_gpu_2d_render",
        config(json!({
            "width": w, "height": h, "msaa": 4,
            "background": [0.0, 0.0, 0.0, 0.0],
            "shapes": [{ "type": "image", "bounds": [0, 0, w, h], "z": 0 }],
            "text": [{
                "content": "Reflow — Melting Ice",
                "x": w as f64 - 200.0, "y": h as f64 - 14.0,
                "size": 14.0, "color": [1.0, 1.0, 1.0, 0.5],
                "tracking": 0.5, "center": false,
                "font": "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
            }],
        })),
    )?;
    net.add_node(
        "collector",
        "tpl_render_frame_collector",
        config(json!({
            "totalFrames": total_frames, "width": w, "height": h, "fps": fps, "logProgress": true,
        })),
    )?;
    net.add_node(
        "encoder",
        "tpl_video_encoder",
        config(json!({ "fps": fps, "bitrate": 20000 })),
    )?;
    net.add_node(
        "save",
        "tpl_file_save",
        config(json!({ "path": "melting_ice.mp4" })),
    )?;
    net.add_connection(wire("render", "output", "composite", "data"));
    net.add_connection(wire("composite", "image", "collector", "frame"));
    net.add_connection(wire(
        "anim_time",
        "frame_number",
        "collector",
        "frame_number",
    ));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    net.add_initial(iip("ice_material", "trigger", Message::Flow));
    net.add_initial(iip("water_material", "trigger", Message::Flow));

    println!(
        "Pipeline: IceMaterial + WaterMaterial → SdfScene ← IceGeometry + PuddleGeometry"
    );
    println!("  {}x{}, {}fps, {} frames\n", w, h, fps, total_frames);
    println!("  maxSteps={}\n", max_steps);

    let event_rx = net.get_event_receiver();
    tokio::spawn(async move {
        let mut render_done_count = 0u64;
        while let Ok(evt) = event_rx.recv_async().await {
            use reflow_network::network::NetworkEvent;
            match &evt {
                NetworkEvent::ActorFailed {
                    actor_id, error, ..
                } => {
                    eprintln!("[FAIL] actor={} err={}", actor_id, error);
                }
                NetworkEvent::MessageSent {
                    from_actor,
                    to_actor,
                    to_port,
                    message,
                    ..
                } if from_actor == "anim_time"
                    && to_actor == "collector"
                    && to_port == "frame_number" =>
                {
                    let value: serde_json::Value = message.clone().into();
                    let frame = value.get("data").and_then(|v| v.as_i64());
                    if let Some(frame) = frame {
                        let frame = frame.max(0) as u64;
                        if frame <= 3 || frame % 6 == 0 || frame as usize >= total_frames {
                            println!("  scheduled frame {}/{}", frame + 1, total_frames);
                        }
                    }
                }
                NetworkEvent::ActorCompleted { actor_id, .. } if actor_id == "render" => {
                    render_done_count += 1;
                    if render_done_count <= 3
                        || render_done_count % 6 == 0
                        || render_done_count as usize >= total_frames
                    {
                        println!("  render complete {}", render_done_count);
                    }
                }
                NetworkEvent::ActorCompleted {
                    actor_id,
                    outputs: Some(outputs),
                    ..
                } if actor_id == "collector" => {
                    let progress = outputs.get("progress");
                    let frame = progress
                        .and_then(|v| v.get("data"))
                        .and_then(|v| v.get("frame"))
                        .and_then(|v| v.as_u64());
                    let total = progress
                        .and_then(|v| v.get("data"))
                        .and_then(|v| v.get("totalFrames"))
                        .and_then(|v| v.as_u64());
                    if let (Some(frame), Some(total)) = (frame, total) {
                        if frame <= 3 || frame % 6 == 0 || frame == total {
                            println!("  frame {}/{}", frame, total);
                        }
                    }
                }
                _ => {}
            }
        }
    });

    let start = std::time::Instant::now();
    net.start()?;

    let mp4 = std::path::Path::new("melting_ice.mp4");
    let timeout_secs = std::env::var("REFLOW_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| (total_frames as u64).saturating_mul(5).max(180));
    let timeout = std::time::Duration::from_secs(timeout_secs);
    println!("  timeout={}s\n", timeout_secs);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if mp4.exists() && mp4.metadata().map(|m| m.len() > 100).unwrap_or(false) {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            break;
        }
        let elapsed = start.elapsed();
        if elapsed.as_secs() % 10 == 0 && elapsed.as_secs() > 0 {
            println!("  {:.0}s...", elapsed.as_secs_f64());
        }
        if elapsed > timeout {
            eprintln!("Timed out");
            break;
        }
    }

    let elapsed = start.elapsed();
    if mp4.exists() {
        let size = std::fs::metadata(mp4)?.len();
        println!(
            "Saved: melting_ice.mp4 ({} bytes, {:.1}s)",
            size,
            elapsed.as_secs_f64()
        );
    }
    std::process::exit(0);
}
