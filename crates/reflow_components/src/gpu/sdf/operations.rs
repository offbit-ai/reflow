//! SDF operation actors — combine two SDF inputs via CSG operations.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_sdf::ir::SdfNode;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

fn parse_sdf(msg: Option<&Message>) -> Option<SdfNode> {
    match msg {
        Some(Message::Object(v)) => {
            let json: serde_json::Value = v.as_ref().clone().into();
            serde_json::from_value(json).ok()
        }
        _ => None,
    }
}

fn sdf_output(node: &SdfNode) -> HashMap<String, Message> {
    let json = serde_json::to_value(node).unwrap_or_default();
    let mut out = HashMap::new();
    out.insert(
        "sdf".to_string(),
        Message::object(EncodableValue::from(json)),
    );
    out
}

fn cache_sdf_input(context: &ActorContext, key: &str) {
    if let Some(node) = parse_sdf(context.get_payload().get(key)) {
        if let Ok(value) = serde_json::to_value(node) {
            context.pool_upsert("_sdf_inputs", key, value);
        }
    }
}

fn cached_binary_inputs(context: &ActorContext) -> Result<Option<(SdfNode, SdfNode)>, Error> {
    cache_sdf_input(context, "sdf_a");
    cache_sdf_input(context, "sdf_b");

    let cache: HashMap<String, Value> = context.get_pool("_sdf_inputs").into_iter().collect();
    let a = cache
        .get("sdf_a")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| anyhow::anyhow!("Invalid cached sdf_a: {}", e))?;
    let b = cache
        .get("sdf_b")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| anyhow::anyhow!("Invalid cached sdf_b: {}", e))?;

    Ok(match (a, b) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    })
}

fn default_stamp_smoothness() -> f32 {
    0.05
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
enum StampOp {
    Union,
    Difference,
    Intersection,
    SmoothUnion,
    SmoothDifference,
    SmoothIntersection,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "shape", rename_all = "camelCase")]
enum StampShape {
    Sphere {
        radius: f32,
    },
    Ellipsoid {
        radii: [f32; 3],
    },
    RoundBox {
        size: [f32; 3],
        radius: f32,
    },
    RoundBoxShell {
        size: [f32; 3],
        radius: f32,
        thickness: f32,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StampSpec {
    #[serde(flatten)]
    shape: StampShape,
    op: StampOp,
    #[serde(default = "default_stamp_smoothness")]
    smoothness: f32,
    #[serde(default)]
    offset: [f32; 3],
    #[serde(default)]
    angles: [f32; 3],
}

fn stamp_shape_node(shape: &StampShape) -> SdfNode {
    match shape {
        StampShape::Sphere { radius } => SdfNode::sphere(*radius),
        StampShape::Ellipsoid { radii } => SdfNode::ellipsoid(*radii),
        StampShape::RoundBox { size, radius } => SdfNode::round_box(*size, *radius),
        StampShape::RoundBoxShell {
            size,
            radius,
            thickness,
        } => SdfNode::round_box_shell(*size, *radius, *thickness),
    }
}

fn transform_stamp(mut node: SdfNode, spec: &StampSpec) -> SdfNode {
    if spec.angles != [0.0, 0.0, 0.0] {
        node = node.rotate(spec.angles);
    }
    if spec.offset != [0.0, 0.0, 0.0] {
        node = node.translate(spec.offset);
    }
    node
}

fn apply_stamp(base: SdfNode, spec: &StampSpec) -> SdfNode {
    let stamp = transform_stamp(stamp_shape_node(&spec.shape), spec);
    match spec.op {
        StampOp::Union => SdfNode::union(base, stamp),
        StampOp::Difference => SdfNode::difference(base, stamp),
        StampOp::Intersection => SdfNode::intersection(base, stamp),
        StampOp::SmoothUnion => SdfNode::smooth_union(base, stamp, spec.smoothness),
        StampOp::SmoothDifference => SdfNode::smooth_difference(base, stamp, spec.smoothness),
        StampOp::SmoothIntersection => SdfNode::smooth_intersection(base, stamp, spec.smoothness),
    }
}

fn parse_stamp_specs(context: &ActorContext) -> Result<Vec<StampSpec>, Error> {
    let config = context.get_config_hashmap();
    let Some(value) = config.get("stamps") else {
        return Ok(Vec::new());
    };
    serde_json::from_value::<Vec<StampSpec>>(value.clone())
        .map_err(|e| anyhow::anyhow!("Invalid stamps config: {}", e))
}

// ── Union ───────────────────────────────────────────────────────

#[actor(SdfUnionActor, inports::<10>(sdf_a, sdf_b), outports::<1>(sdf, error), state(MemoryState), await_all_inports)]
pub async fn sdf_union_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let Some((a, b)) = cached_binary_inputs(&context)? else {
        return Ok(HashMap::new());
    };
    Ok(sdf_output(&SdfNode::union(a, b)))
}

// ── Intersection ────────────────────────────────────────────────

#[actor(SdfIntersectionActor, inports::<10>(sdf_a, sdf_b), outports::<1>(sdf, error), state(MemoryState), await_all_inports)]
pub async fn sdf_intersection_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let Some((a, b)) = cached_binary_inputs(&context)? else {
        return Ok(HashMap::new());
    };
    Ok(sdf_output(&SdfNode::intersection(a, b)))
}

// ── Difference ──────────────────────────────────────────────────

#[actor(SdfDifferenceActor, inports::<10>(sdf_a, sdf_b), outports::<1>(sdf, error), state(MemoryState), await_all_inports)]
pub async fn sdf_difference_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let Some((a, b)) = cached_binary_inputs(&context)? else {
        return Ok(HashMap::new());
    };
    Ok(sdf_output(&SdfNode::difference(a, b)))
}

// ── Smooth Union ────────────────────────────────────────────────

#[actor(SdfSmoothUnionActor, inports::<10>(sdf_a, sdf_b), outports::<1>(sdf, error), state(MemoryState), await_all_inports)]
pub async fn sdf_smooth_union_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let k = config
        .get("smoothness")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3) as f32;
    let Some((a, b)) = cached_binary_inputs(&context)? else {
        return Ok(HashMap::new());
    };
    Ok(sdf_output(&SdfNode::smooth_union(a, b, k)))
}

// ── Smooth Intersection ─────────────────────────────────────────

#[actor(SdfSmoothIntersectionActor, inports::<10>(sdf_a, sdf_b), outports::<1>(sdf, error), state(MemoryState), await_all_inports)]
pub async fn sdf_smooth_intersection_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let k = config
        .get("smoothness")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3) as f32;
    let Some((a, b)) = cached_binary_inputs(&context)? else {
        return Ok(HashMap::new());
    };
    Ok(sdf_output(&SdfNode::smooth_intersection(a, b, k)))
}

// ── Smooth Difference ───────────────────────────────────────────

#[actor(SdfSmoothDifferenceActor, inports::<10>(sdf_a, sdf_b), outports::<1>(sdf, error), state(MemoryState), await_all_inports)]
pub async fn sdf_smooth_difference_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let k = config
        .get("smoothness")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3) as f32;
    let Some((a, b)) = cached_binary_inputs(&context)? else {
        return Ok(HashMap::new());
    };
    Ok(sdf_output(&SdfNode::smooth_difference(a, b, k)))
}

// ── Stamp Compose ────────────────────────────────────────────────

#[actor(SdfStampComposeActor, inports::<10>(sdf), outports::<1>(sdf, error), state(MemoryState))]
pub async fn sdf_stamp_compose_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let base = parse_sdf(payload.get("sdf")).ok_or_else(|| anyhow::anyhow!("Missing sdf input"))?;
    let stamps = parse_stamp_specs(&context)?;

    if stamps.is_empty() {
        return Ok(sdf_output(&base));
    }

    let composed = stamps.iter().fold(base, apply_stamp);
    Ok(sdf_output(&composed))
}
