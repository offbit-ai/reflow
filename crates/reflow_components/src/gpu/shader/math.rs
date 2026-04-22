//! Shader math and utility nodes.

use super::{cached_shader_input, connected_shader_inputs_ready, update_shader_input_cache};
use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(ShaderMathActor, inports::<10>(a, b), outports::<1>(shader), state(MemoryState))]
pub async fn shader_math_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let cache = update_shader_input_cache(&ctx, &["a", "b"]);
    if !connected_shader_inputs_ready(&ctx, &cache, &["a", "b"]) {
        return Ok(HashMap::new());
    }
    let op = config.get("op").and_then(|v| v.as_str()).unwrap_or("add");
    let a = cached_shader_input(&cache, "a").unwrap_or(json!({"type": "constFloat", "c": 0.0}));
    let b = cached_shader_input(&cache, "b");
    let mut node = json!({"type": "mathOp", "op": op, "a": a});
    if let Some(bv) = b {
        node["b"] = bv;
    }
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(node)),
    );
    Ok(out)
}

#[actor(ShaderColorMixActor, inports::<10>(fac, a, b), outports::<1>(shader), state(MemoryState))]
pub async fn shader_color_mix_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let cache = update_shader_input_cache(&ctx, &["fac", "a", "b"]);
    if !connected_shader_inputs_ready(&ctx, &cache, &["fac", "a", "b"]) {
        return Ok(HashMap::new());
    }
    let mode = config.get("mode").and_then(|v| v.as_str()).unwrap_or("mix");
    fn get_ir(cache: &HashMap<String, Value>, port: &str, default: Value) -> Value {
        cached_shader_input(cache, port).unwrap_or(default)
    }
    let fac = get_ir(
        &cache,
        "fac",
        config
            .get("fac")
            .cloned()
            .unwrap_or(json!({"type": "constFloat", "c": 0.5})),
    );
    let a = get_ir(
        &cache,
        "a",
        config
            .get("a")
            .cloned()
            .unwrap_or(json!({"type": "constVec3", "c": [0.0, 0.0, 0.0]})),
    );
    let b = get_ir(
        &cache,
        "b",
        config
            .get("b")
            .cloned()
            .unwrap_or(json!({"type": "constVec3", "c": [1.0, 1.0, 1.0]})),
    );
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": "colorMix", "mode": mode, "fac": fac, "a": a, "b": b,
        }))),
    );
    Ok(out)
}

#[actor(ShaderColorRampActor, inports::<10>(input), outports::<1>(shader), state(MemoryState))]
pub async fn shader_color_ramp_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let stops = config.get("stops").cloned().unwrap_or(json!([
        {"position": 0.0, "color": [0.0, 0.0, 0.0, 1.0]},
        {"position": 1.0, "color": [1.0, 1.0, 1.0, 1.0]},
    ]));
    let input = match payload.get("input") {
        Some(Message::Object(obj)) => {
            let v: Value = obj.as_ref().clone().into();
            v
        }
        _ => json!({"type": "constFloat", "c": 0.5}),
    };
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": "colorRamp", "stops": stops, "input": input,
        }))),
    );
    Ok(out)
}

#[actor(ShaderFresnelActor, inports::<10>(ior), outports::<1>(shader), state(MemoryState))]
pub async fn shader_fresnel_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let ior = match payload.get("ior") {
        Some(Message::Object(obj)) => {
            let v: Value = obj.as_ref().clone().into();
            v
        }
        _ => {
            json!({"type": "constFloat", "c": config.get("ior").and_then(|v| v.as_f64()).unwrap_or(1.45)})
        }
    };
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": "fresnel", "ior": ior,
        }))),
    );
    Ok(out)
}
