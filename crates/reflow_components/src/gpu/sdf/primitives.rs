//! SDF primitive actors — output SdfNode IR as Message::Object.
//!
//! Each actor reads its config properties and produces an SdfNode
//! representing a primitive shape. Downstream operation/transform actors
//! combine them into a tree.
//!
//! Primitives are source actors — they need a `trigger` IIP to execute.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use std::collections::HashMap;
use std::convert::TryInto;

fn sdf_output(node: &reflow_sdf::ir::SdfNode) -> HashMap<String, Message> {
    let json = serde_json::to_value(node).unwrap_or_default();
    let mut out = HashMap::new();
    out.insert(
        "sdf".to_string(),
        Message::object(EncodableValue::from(json)),
    );
    out
}

fn config_f32(config: &HashMap<String, serde_json::Value>, key: &str, default: f32) -> f32 {
    config
        .get(key)
        .and_then(|v| v.as_f64())
        .unwrap_or(default as f64) as f32
}

fn parse_vec3(value: &serde_json::Value) -> Option<[f32; 3]> {
    let arr = value.as_array()?;
    let vals: Vec<f32> = arr
        .iter()
        .map(|entry| entry.as_f64().map(|v| v as f32))
        .collect::<Option<Vec<_>>>()?;
    vals.try_into().ok()
}

fn parse_vec3_list(
    config: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Option<Vec<[f32; 3]>> {
    let arr = config.get(key)?.as_array()?;
    arr.iter().map(parse_vec3).collect()
}

fn parse_f32_list(config: &HashMap<String, serde_json::Value>, key: &str) -> Option<Vec<f32>> {
    let arr = config.get(key)?.as_array()?;
    arr.iter()
        .map(|entry| entry.as_f64().map(|v| v as f32))
        .collect()
}

#[actor(SdfSphereActor, inports::<1>(), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_sphere_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let radius = config.get("radius").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::sphere(radius)))
}

#[actor(SdfBoxActor, inports::<1>(), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_box_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let sx = config_f32(&config, "sizeX", 1.0);
    let sy = config_f32(&config, "sizeY", 1.0);
    let sz = config_f32(&config, "sizeZ", 1.0);
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::box3([sx, sy, sz])))
}

#[actor(SdfRoundBoxActor, inports::<1>(), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_round_box_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let sx = config_f32(&config, "sizeX", 1.0);
    let sy = config_f32(&config, "sizeY", 1.0);
    let sz = config_f32(&config, "sizeZ", 1.0);
    let radius = config_f32(&config, "radius", 0.05);
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::round_box(
        [sx, sy, sz],
        radius,
    )))
}

#[actor(SdfEllipsoidActor, inports::<1>(), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_ellipsoid_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let rx = config_f32(&config, "radiusX", 1.0);
    let ry = config_f32(&config, "radiusY", 1.0);
    let rz = config_f32(&config, "radiusZ", 1.0);
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::ellipsoid([
        rx, ry, rz,
    ])))
}

#[actor(
    SdfRoundBoxShellActor,
    inports::<1>(),
    outports::<1>(sdf),
    state(MemoryState)
)]
pub async fn sdf_round_box_shell_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let sx = config_f32(&config, "sizeX", 1.0);
    let sy = config_f32(&config, "sizeY", 1.0);
    let sz = config_f32(&config, "sizeZ", 1.0);
    let radius = config_f32(&config, "radius", 0.05);
    let thickness = config_f32(&config, "thickness", 0.08);
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::round_box_shell(
        [sx, sy, sz],
        radius,
        thickness,
    )))
}

#[actor(SdfCylinderActor, inports::<1>(), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_cylinder_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let radius = config.get("radius").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
    let height = config.get("height").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::cylinder(
        radius, height,
    )))
}

#[actor(SdfTorusActor, inports::<1>(), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_torus_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let major = config
        .get("majorRadius")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;
    let minor = config
        .get("minorRadius")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::torus(major, minor)))
}

#[actor(SdfCapsuleActor, inports::<1>(), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_capsule_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let radius = config.get("radius").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
    let height = config.get("height").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::capsule(
        radius, height,
    )))
}

#[actor(SdfConeActor, inports::<1>(), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_cone_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let angle = config.get("angle").and_then(|v| v.as_f64()).unwrap_or(30.0) as f32;
    let height = config.get("height").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::cone(
        angle.to_radians(),
        height,
    )))
}

#[actor(
    SdfTaperedCapsuleActor,
    inports::<1>(),
    outports::<1>(sdf),
    state(MemoryState)
)]
pub async fn sdf_tapered_capsule_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let a = [
        config_f32(&config, "ax", 0.0),
        config_f32(&config, "ay", 0.0),
        config_f32(&config, "az", 0.0),
    ];
    let b = [
        config_f32(&config, "bx", 0.0),
        config_f32(&config, "by", 1.0),
        config_f32(&config, "bz", 0.0),
    ];
    let radius_a = config_f32(&config, "radiusA", 0.3);
    let radius_b = config_f32(&config, "radiusB", 0.2);
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::tapered_capsule(
        a, b, radius_a, radius_b,
    )))
}

#[actor(SdfPlaneActor, inports::<1>(), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_plane_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let nx = config
        .get("normalX")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    let ny = config
        .get("normalY")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;
    let nz = config
        .get("normalZ")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    let offset = config.get("offset").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::plane(
        [nx, ny, nz],
        offset,
    )))
}

#[actor(SdfInfRepeatActor, inports::<1>(), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_inf_repeat_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let sx = config_f32(&config, "spacingX", 2.0);
    let sy = config_f32(&config, "spacingY", 2.0);
    let sz = config_f32(&config, "spacingZ", 2.0);
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::inf_repeat([
        sx, sy, sz,
    ])))
}

#[actor(SdfTubePathActor, inports::<1>(), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_tube_path_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let points = parse_vec3_list(&config, "points")
        .ok_or_else(|| anyhow::anyhow!("Missing points array of [x, y, z] values"))?;
    let radii = parse_f32_list(&config, "radii")
        .ok_or_else(|| anyhow::anyhow!("Missing radii array of float values"))?;
    let smoothness = config_f32(&config, "smoothness", 0.1);
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::tube_path(
        points, radii, smoothness,
    )))
}

#[actor(SdfPuddleActor, inports::<1>(), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_puddle_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let radius = config.get("radius").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let height = config
        .get("height")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.01) as f32;
    let noise_freq = config
        .get("noiseFreq")
        .and_then(|v| v.as_f64())
        .unwrap_or(2.0) as f32;
    let noise_amp = config
        .get("noiseAmp")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3) as f32;
    let bevel = config
        .get("bevel")
        .and_then(|v| v.as_f64())
        .unwrap_or((radius * 0.12).max(height * 6.0) as f64) as f32;
    let meniscus = config
        .get("meniscus")
        .and_then(|v| v.as_f64())
        .unwrap_or((height * 0.35) as f64) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::puddle_profiled(
        radius, height, noise_freq, noise_amp, bevel, meniscus,
    )))
}
