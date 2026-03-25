//! Shader effect nodes — normal map, bump map, mapping, separate/combine XYZ.

use super::{cached_shader_input, connected_shader_inputs_ready, update_shader_input_cache};
use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(ShaderNormalMapActor, inports::<10>(strength, color), outports::<1>(shader), state(MemoryState))]
pub async fn shader_normal_map_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let cache = update_shader_input_cache(&ctx, &["strength", "color"]);
    if !connected_shader_inputs_ready(&ctx, &cache, &["strength", "color"]) {
        return Ok(HashMap::new());
    }
    fn get_ir(
        cache: &HashMap<String, Value>,
        c: &HashMap<String, Value>,
        k: &str,
        d: Value,
    ) -> Value {
        cached_shader_input(cache, k)
            .or_else(|| c.get(k).cloned())
            .unwrap_or(d)
    }
    let strength = get_ir(
        &cache,
        &config,
        "strength",
        json!({"type": "constFloat", "c": 1.0}),
    );
    let color = get_ir(
        &cache,
        &config,
        "color",
        json!({"type": "constVec3", "c": [0.5, 0.5, 1.0]}),
    );
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": "normalMap", "strength": strength, "color": color,
        }))),
    );
    Ok(out)
}

#[actor(ShaderBumpMapActor, inports::<10>(strength, height), outports::<1>(shader), state(MemoryState))]
pub async fn shader_bump_map_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let cache = update_shader_input_cache(&ctx, &["strength", "height"]);
    if !connected_shader_inputs_ready(&ctx, &cache, &["strength", "height"]) {
        return Ok(HashMap::new());
    }
    fn get_ir(
        cache: &HashMap<String, Value>,
        c: &HashMap<String, Value>,
        k: &str,
        d: Value,
    ) -> Value {
        cached_shader_input(cache, k)
            .or_else(|| c.get(k).cloned())
            .unwrap_or(d)
    }
    let strength = get_ir(
        &cache,
        &config,
        "strength",
        json!({"type": "constFloat", "c": 1.0}),
    );
    let height = get_ir(
        &cache,
        &config,
        "height",
        json!({"type": "constFloat", "c": 0.0}),
    );
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": "bumpMap", "strength": strength, "height": height,
        }))),
    );
    Ok(out)
}

#[actor(ShaderMappingActor, inports::<10>(input), outports::<1>(shader), state(MemoryState))]
pub async fn shader_mapping_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let input = match payload.get("input") {
        Some(Message::Object(o)) => {
            let v: Value = o.as_ref().clone().into();
            v
        }
        _ => json!({"type": "texCoord"}),
    };
    let location = config
        .get("location")
        .and_then(|v| v.as_array())
        .map(|a| {
            [
                a.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            ]
        })
        .unwrap_or([0.0; 3]);
    let rotation = config
        .get("rotation")
        .and_then(|v| v.as_array())
        .map(|a| {
            [
                a.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            ]
        })
        .unwrap_or([0.0; 3]);
    let scale = config
        .get("scale")
        .and_then(|v| v.as_array())
        .map(|a| {
            [
                a.get(0).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
            ]
        })
        .unwrap_or([1.0; 3]);
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": "mapping", "input": input,
            "location": location, "rotation": rotation, "scale": scale,
        }))),
    );
    Ok(out)
}

#[actor(ShaderSeparateXYZActor, inports::<10>(input), outports::<1>(x, y, z), state(MemoryState))]
pub async fn shader_separate_xyz_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let input = match payload.get("input") {
        Some(Message::Object(o)) => {
            let v: Value = o.as_ref().clone().into();
            v
        }
        _ => json!({"type": "objectPosition"}),
    };
    let mut out = HashMap::new();
    out.insert(
        "x".to_string(),
        Message::object(EncodableValue::from(
            json!({"type": "separateXYZ", "input": input, "component": "x"}),
        )),
    );
    out.insert(
        "y".to_string(),
        Message::object(EncodableValue::from(
            json!({"type": "separateXYZ", "input": input, "component": "y"}),
        )),
    );
    out.insert(
        "z".to_string(),
        Message::object(EncodableValue::from(
            json!({"type": "separateXYZ", "input": input, "component": "z"}),
        )),
    );
    Ok(out)
}

#[actor(ShaderCombineXYZActor, inports::<10>(x, y, z), outports::<1>(shader), state(MemoryState))]
pub async fn shader_combine_xyz_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let cache = update_shader_input_cache(&ctx, &["x", "y", "z"]);
    if !connected_shader_inputs_ready(&ctx, &cache, &["x", "y", "z"]) {
        return Ok(HashMap::new());
    }
    fn get_ir(cache: &HashMap<String, Value>, k: &str) -> Value {
        cached_shader_input(cache, k).unwrap_or(json!({"type": "constFloat", "c": 0.0}))
    }
    let mut out = HashMap::new();
    out.insert("shader".to_string(), Message::object(EncodableValue::from(json!({
        "type": "combineXYZ", "x": get_ir(&cache, "x"), "y": get_ir(&cache, "y"), "z": get_ir(&cache, "z"),
    }))));
    Ok(out)
}

#[actor(ShaderClampActor, inports::<10>(input), outports::<1>(shader), state(MemoryState))]
pub async fn shader_clamp_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let input = match payload.get("input") {
        Some(Message::Object(o)) => {
            let v: Value = o.as_ref().clone().into();
            v
        }
        _ => json!({"type": "constFloat", "c": 0.5}),
    };
    let min_val = config.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let max_val = config.get("max").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": "clamp", "input": input, "minVal": min_val, "maxVal": max_val,
        }))),
    );
    Ok(out)
}

#[actor(ShaderMapRangeActor, inports::<10>(input), outports::<1>(shader), state(MemoryState))]
pub async fn shader_map_range_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let input = match payload.get("input") {
        Some(Message::Object(o)) => {
            let v: Value = o.as_ref().clone().into();
            v
        }
        _ => json!({"type": "constFloat", "c": 0.5}),
    };
    let from_min = config
        .get("fromMin")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    let from_max = config
        .get("fromMax")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;
    let to_min = config.get("toMin").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let to_max = config.get("toMax").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": "mapRange", "input": input,
            "fromMin": from_min, "fromMax": from_max, "toMin": to_min, "toMax": to_max,
        }))),
    );
    Ok(out)
}
