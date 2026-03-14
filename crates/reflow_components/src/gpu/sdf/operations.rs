//! SDF operation actors — combine two SDF inputs via CSG operations.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_sdf::ir::SdfNode;
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
    out.insert("sdf".to_string(), Message::object(EncodableValue::from(json)));
    out
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}

// ── Union ───────────────────────────────────────────────────────

#[actor(SdfUnionActor, inports::<10>(sdf_a, sdf_b), outports::<1>(sdf, error), state(MemoryState), await_all_inports)]
pub async fn sdf_union_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let a = parse_sdf(payload.get("sdf_a")).ok_or_else(|| anyhow::anyhow!("Missing sdf_a"))?;
    let b = parse_sdf(payload.get("sdf_b")).ok_or_else(|| anyhow::anyhow!("Missing sdf_b"))?;
    Ok(sdf_output(&SdfNode::union(a, b)))
}

// ── Intersection ────────────────────────────────────────────────

#[actor(SdfIntersectionActor, inports::<10>(sdf_a, sdf_b), outports::<1>(sdf, error), state(MemoryState), await_all_inports)]
pub async fn sdf_intersection_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let a = parse_sdf(payload.get("sdf_a")).ok_or_else(|| anyhow::anyhow!("Missing sdf_a"))?;
    let b = parse_sdf(payload.get("sdf_b")).ok_or_else(|| anyhow::anyhow!("Missing sdf_b"))?;
    Ok(sdf_output(&SdfNode::intersection(a, b)))
}

// ── Difference ──────────────────────────────────────────────────

#[actor(SdfDifferenceActor, inports::<10>(sdf_a, sdf_b), outports::<1>(sdf, error), state(MemoryState), await_all_inports)]
pub async fn sdf_difference_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let a = parse_sdf(payload.get("sdf_a")).ok_or_else(|| anyhow::anyhow!("Missing sdf_a"))?;
    let b = parse_sdf(payload.get("sdf_b")).ok_or_else(|| anyhow::anyhow!("Missing sdf_b"))?;
    Ok(sdf_output(&SdfNode::difference(a, b)))
}

// ── Smooth Union ────────────────────────────────────────────────

#[actor(SdfSmoothUnionActor, inports::<10>(sdf_a, sdf_b), outports::<1>(sdf, error), state(MemoryState), await_all_inports)]
pub async fn sdf_smooth_union_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let k = config.get("smoothness").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
    let a = parse_sdf(payload.get("sdf_a")).ok_or_else(|| anyhow::anyhow!("Missing sdf_a"))?;
    let b = parse_sdf(payload.get("sdf_b")).ok_or_else(|| anyhow::anyhow!("Missing sdf_b"))?;
    Ok(sdf_output(&SdfNode::smooth_union(a, b, k)))
}

// ── Smooth Intersection ─────────────────────────────────────────

#[actor(SdfSmoothIntersectionActor, inports::<10>(sdf_a, sdf_b), outports::<1>(sdf, error), state(MemoryState), await_all_inports)]
pub async fn sdf_smooth_intersection_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let k = config.get("smoothness").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
    let a = parse_sdf(payload.get("sdf_a")).ok_or_else(|| anyhow::anyhow!("Missing sdf_a"))?;
    let b = parse_sdf(payload.get("sdf_b")).ok_or_else(|| anyhow::anyhow!("Missing sdf_b"))?;
    Ok(sdf_output(&SdfNode::smooth_intersection(a, b, k)))
}

// ── Smooth Difference ───────────────────────────────────────────

#[actor(SdfSmoothDifferenceActor, inports::<10>(sdf_a, sdf_b), outports::<1>(sdf, error), state(MemoryState), await_all_inports)]
pub async fn sdf_smooth_difference_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let k = config.get("smoothness").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
    let a = parse_sdf(payload.get("sdf_a")).ok_or_else(|| anyhow::anyhow!("Missing sdf_a"))?;
    let b = parse_sdf(payload.get("sdf_b")).ok_or_else(|| anyhow::anyhow!("Missing sdf_b"))?;
    Ok(sdf_output(&SdfNode::smooth_difference(a, b, k)))
}
