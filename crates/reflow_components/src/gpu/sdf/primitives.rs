//! SDF primitive actors — output SdfNode IR as Message::Object.
//!
//! Each actor reads its config properties and produces an SdfNode
//! representing a primitive shape. Downstream operation/transform actors
//! combine them into a tree.
//!
//! Primitives are source actors — they need a `trigger` IIP to execute.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use std::collections::HashMap;

fn sdf_output(node: &reflow_sdf::ir::SdfNode) -> HashMap<String, Message> {
    let json = serde_json::to_value(node).unwrap_or_default();
    let mut out = HashMap::new();
    out.insert("sdf".to_string(), Message::object(EncodableValue::from(json)));
    out
}

#[actor(SdfSphereActor, inports::<1>(trigger), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_sphere_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let radius = config.get("radius").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::sphere(radius)))
}

#[actor(SdfBoxActor, inports::<1>(trigger), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_box_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let sx = config.get("sizeX").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let sy = config.get("sizeY").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let sz = config.get("sizeZ").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::box3([sx, sy, sz])))
}

#[actor(SdfCylinderActor, inports::<1>(trigger), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_cylinder_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let radius = config.get("radius").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
    let height = config.get("height").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::cylinder(radius, height)))
}

#[actor(SdfTorusActor, inports::<1>(trigger), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_torus_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let major = config.get("majorRadius").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let minor = config.get("minorRadius").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::torus(major, minor)))
}

#[actor(SdfCapsuleActor, inports::<1>(trigger), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_capsule_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let radius = config.get("radius").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
    let height = config.get("height").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::capsule(radius, height)))
}

#[actor(SdfConeActor, inports::<1>(trigger), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_cone_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let angle = config.get("angle").and_then(|v| v.as_f64()).unwrap_or(30.0) as f32;
    let height = config.get("height").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::cone(angle.to_radians(), height)))
}

#[actor(SdfPlaneActor, inports::<1>(trigger), outports::<1>(sdf), state(MemoryState))]
pub async fn sdf_plane_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let nx = config.get("normalX").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let ny = config.get("normalY").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let nz = config.get("normalZ").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let offset = config.get("offset").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    Ok(sdf_output(&reflow_sdf::ir::SdfNode::plane([nx, ny, nz], offset)))
}
