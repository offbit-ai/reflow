//! SDF transform actors — wrap a child SDF in a domain transform.

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
    out.insert(
        "sdf".to_string(),
        Message::object(EncodableValue::from(json)),
    );
    out
}

// ── Translate ───────────────────────────────────────────────────

#[actor(SdfTranslateActor, inports::<10>(sdf), outports::<1>(sdf, error), state(MemoryState))]
pub async fn sdf_translate_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let child =
        parse_sdf(payload.get("sdf")).ok_or_else(|| anyhow::anyhow!("Missing sdf input"))?;
    let x = config.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let y = config.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let z = config.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    Ok(sdf_output(&child.translate([x, y, z])))
}

// ── Rotate ──────────────────────────────────────────────────────

#[actor(SdfRotateActor, inports::<10>(sdf), outports::<1>(sdf, error), state(MemoryState))]
pub async fn sdf_rotate_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let child =
        parse_sdf(payload.get("sdf")).ok_or_else(|| anyhow::anyhow!("Missing sdf input"))?;
    let x = config.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let y = config.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let z = config.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    Ok(sdf_output(&child.rotate([x, y, z])))
}

// ── Scale ───────────────────────────────────────────────────────

#[actor(SdfScaleActor, inports::<10>(sdf), outports::<1>(sdf, error), state(MemoryState))]
pub async fn sdf_scale_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let child =
        parse_sdf(payload.get("sdf")).ok_or_else(|| anyhow::anyhow!("Missing sdf input"))?;
    let uniform = config.get("factor").and_then(|v| v.as_f64());
    let sx = config.get("factorX").and_then(|v| v.as_f64()).or(uniform).unwrap_or(1.0) as f32;
    let sy = config.get("factorY").and_then(|v| v.as_f64()).or(uniform).unwrap_or(1.0) as f32;
    let sz = config.get("factorZ").and_then(|v| v.as_f64()).or(uniform).unwrap_or(1.0) as f32;
    Ok(sdf_output(&child.scale([sx, sy, sz])))
}

// ── Twist ───────────────────────────────────────────────────────

#[actor(SdfTwistActor, inports::<10>(sdf), outports::<1>(sdf, error), state(MemoryState))]
pub async fn sdf_twist_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let child =
        parse_sdf(payload.get("sdf")).ok_or_else(|| anyhow::anyhow!("Missing sdf input"))?;
    let strength = config
        .get("strength")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5) as f32;
    Ok(sdf_output(&child.twist(strength)))
}

// ── Bend ────────────────────────────────────────────────────────

#[actor(SdfBendActor, inports::<10>(sdf), outports::<1>(sdf, error), state(MemoryState))]
pub async fn sdf_bend_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let child =
        parse_sdf(payload.get("sdf")).ok_or_else(|| anyhow::anyhow!("Missing sdf input"))?;
    let strength = config
        .get("strength")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5) as f32;
    Ok(sdf_output(&child.bend(strength)))
}

// ── Round ───────────────────────────────────────────────────────

#[actor(SdfRoundActor, inports::<10>(sdf), outports::<1>(sdf, error), state(MemoryState))]
pub async fn sdf_round_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let child =
        parse_sdf(payload.get("sdf")).ok_or_else(|| anyhow::anyhow!("Missing sdf input"))?;
    let radius = config
        .get("radius")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.05) as f32;
    Ok(sdf_output(&child.round(radius)))
}

// ── Shell ───────────────────────────────────────────────────────

#[actor(SdfShellActor, inports::<10>(sdf), outports::<1>(sdf, error), state(MemoryState))]
pub async fn sdf_shell_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let child =
        parse_sdf(payload.get("sdf")).ok_or_else(|| anyhow::anyhow!("Missing sdf input"))?;
    let thickness = config
        .get("thickness")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.05) as f32;
    Ok(sdf_output(&child.shell(thickness)))
}

// ── Mirror ──────────────────────────────────────────────────────

#[actor(SdfMirrorActor, inports::<10>(sdf), outports::<1>(sdf, error), state(MemoryState))]
pub async fn sdf_mirror_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let child =
        parse_sdf(payload.get("sdf")).ok_or_else(|| anyhow::anyhow!("Missing sdf input"))?;
    let x = config.get("axisX").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let y = config.get("axisY").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let z = config.get("axisZ").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    Ok(sdf_output(&child.mirror([x, y, z])))
}

// ── Repeat ──────────────────────────────────────────────────────

#[actor(SdfRepeatActor, inports::<10>(sdf), outports::<1>(sdf, error), state(MemoryState))]
pub async fn sdf_repeat_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let child =
        parse_sdf(payload.get("sdf")).ok_or_else(|| anyhow::anyhow!("Missing sdf input"))?;
    let sx = config
        .get("spacingX")
        .and_then(|v| v.as_f64())
        .unwrap_or(2.0) as f32;
    let sy = config
        .get("spacingY")
        .and_then(|v| v.as_f64())
        .unwrap_or(2.0) as f32;
    let sz = config
        .get("spacingZ")
        .and_then(|v| v.as_f64())
        .unwrap_or(2.0) as f32;
    let cx = config.get("countX").and_then(|v| v.as_u64()).unwrap_or(3) as u32;
    let cy = config.get("countY").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
    let cz = config.get("countZ").and_then(|v| v.as_u64()).unwrap_or(3) as u32;
    Ok(sdf_output(&child.repeat([sx, sy, sz], [cx, cy, cz])))
}

// ── Displace (Noise) ────────────────────────────────────────────

#[actor(SdfDisplaceActor, inports::<10>(sdf), outports::<1>(sdf, error), state(MemoryState))]
pub async fn sdf_displace_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let child =
        parse_sdf(payload.get("sdf")).ok_or_else(|| anyhow::anyhow!("Missing sdf input"))?;
    let freq = config
        .get("frequency")
        .and_then(|v| v.as_f64())
        .unwrap_or(3.0) as f32;
    let amp = config
        .get("amplitude")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.1) as f32;
    let oct = config.get("octaves").and_then(|v| v.as_u64()).unwrap_or(4) as u32;
    Ok(sdf_output(&child.displace(freq, amp, oct)))
}
