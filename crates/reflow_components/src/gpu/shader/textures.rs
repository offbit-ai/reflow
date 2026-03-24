//! Shader texture nodes — image textures and procedural textures.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    ShaderImageTextureActor,
    inports::<10>(uv),
    outports::<1>(shader),
    state(MemoryState)
)]
pub async fn shader_image_texture_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let asset_id = config.get("assetId").and_then(|v| v.as_str()).unwrap_or("default");
    let uv = match payload.get("uv") {
        Some(Message::Object(obj)) => { let v: Value = obj.as_ref().clone().into(); v }
        _ => json!({"type": "texCoord"}),
    };
    let mut out = HashMap::new();
    out.insert("shader".to_string(), Message::object(EncodableValue::from(json!({
        "type": "imageTexture",
        "assetId": asset_id,
        "uv": uv,
    }))));
    Ok(out)
}

#[actor(
    ShaderNoiseTextureActor,
    inports::<10>(scale, detail, roughness),
    outports::<1>(shader),
    state(MemoryState)
)]
pub async fn shader_noise_texture_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    fn ir_or_default(payload: &HashMap<String, Message>, config: &HashMap<String, Value>, port: &str, def: f64) -> Value {
        if let Some(Message::Object(obj)) = payload.get(port) { obj.as_ref().clone().into() }
        else { json!({"type": "constFloat", "c": config.get(port).and_then(|v| v.as_f64()).unwrap_or(def)}) }
    }
    let mut out = HashMap::new();
    out.insert("shader".to_string(), Message::object(EncodableValue::from(json!({
        "type": "noiseTexture",
        "scale": ir_or_default(&payload, &config, "scale", 5.0),
        "detail": ir_or_default(&payload, &config, "detail", 2.0),
        "roughness": ir_or_default(&payload, &config, "roughness", 0.5),
    }))));
    Ok(out)
}

#[actor(
    ShaderCheckerTextureActor,
    inports::<10>(scale, color1, color2),
    outports::<1>(shader),
    state(MemoryState)
)]
pub async fn shader_checker_texture_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    fn ir_or_float(payload: &HashMap<String, Message>, config: &HashMap<String, Value>, port: &str, def: f64) -> Value {
        if let Some(Message::Object(obj)) = payload.get(port) { obj.as_ref().clone().into() }
        else { json!({"type": "constFloat", "c": config.get(port).and_then(|v| v.as_f64()).unwrap_or(def)}) }
    }
    fn ir_or_color(payload: &HashMap<String, Message>, config: &HashMap<String, Value>, port: &str, def: [f64;3]) -> Value {
        if let Some(Message::Object(obj)) = payload.get(port) { obj.as_ref().clone().into() }
        else { json!({"type": "constVec3", "c": config.get(port).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>()).unwrap_or(def.to_vec())}) }
    }
    let mut out = HashMap::new();
    out.insert("shader".to_string(), Message::object(EncodableValue::from(json!({
        "type": "checkerTexture",
        "scale": ir_or_float(&payload, &config, "scale", 5.0),
        "color1": ir_or_color(&payload, &config, "color1", [0.8, 0.8, 0.8]),
        "color2": ir_or_color(&payload, &config, "color2", [0.2, 0.2, 0.2]),
    }))));
    Ok(out)
}

#[actor(ShaderVoronoiTextureActor, inports::<10>(scale, randomness), outports::<1>(shader), state(MemoryState))]
pub async fn shader_voronoi_texture_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    fn ir_or_f(p: &HashMap<String, Message>, c: &HashMap<String, Value>, k: &str, d: f64) -> Value {
        if let Some(Message::Object(o)) = p.get(k) { o.as_ref().clone().into() }
        else { json!({"type": "constFloat", "c": c.get(k).and_then(|v| v.as_f64()).unwrap_or(d)}) }
    }
    let mut out = HashMap::new();
    out.insert("shader".to_string(), Message::object(EncodableValue::from(json!({
        "type": "voronoiTexture",
        "scale": ir_or_f(&payload, &config, "scale", 5.0),
        "randomness": ir_or_f(&payload, &config, "randomness", 1.0),
    }))));
    Ok(out)
}

#[actor(ShaderGradientTextureActor, inports::<1>(_trigger), outports::<1>(shader), state(MemoryState))]
pub async fn shader_gradient_texture_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let gradient_type = config.get("gradientType").and_then(|v| v.as_str()).unwrap_or("linear");
    let mut out = HashMap::new();
    out.insert("shader".to_string(), Message::object(EncodableValue::from(json!({
        "type": "gradientTexture", "gradientType": gradient_type,
    }))));
    Ok(out)
}

#[actor(ShaderBrickTextureActor, inports::<10>(scale, mortar_size), outports::<1>(shader), state(MemoryState))]
pub async fn shader_brick_texture_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    fn ir_or_f(p: &HashMap<String, Message>, c: &HashMap<String, Value>, k: &str, d: f64) -> Value {
        if let Some(Message::Object(o)) = p.get(k) { o.as_ref().clone().into() }
        else { json!({"type": "constFloat", "c": c.get(k).and_then(|v| v.as_f64()).unwrap_or(d)}) }
    }
    fn ir_or_c(p: &HashMap<String, Message>, c: &HashMap<String, Value>, k: &str, d: [f64;3]) -> Value {
        if let Some(Message::Object(o)) = p.get(k) { o.as_ref().clone().into() }
        else { json!({"type": "constVec3", "c": d}) }
    }
    let mut out = HashMap::new();
    out.insert("shader".to_string(), Message::object(EncodableValue::from(json!({
        "type": "brickTexture",
        "scale": ir_or_f(&payload, &config, "scale", 5.0),
        "mortarSize": ir_or_f(&payload, &config, "mortar_size", 0.02),
        "color1": ir_or_c(&payload, &config, "color1", [0.8, 0.3, 0.2]),
        "color2": ir_or_c(&payload, &config, "color2", [0.6, 0.2, 0.15]),
        "mortarColor": ir_or_c(&payload, &config, "mortar_color", [0.9, 0.9, 0.85]),
    }))));
    Ok(out)
}

#[actor(ShaderMusgraveTextureActor, inports::<10>(scale, detail, dimension), outports::<1>(shader), state(MemoryState))]
pub async fn shader_musgrave_texture_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    fn ir_or_f(p: &HashMap<String, Message>, c: &HashMap<String, Value>, k: &str, d: f64) -> Value {
        if let Some(Message::Object(o)) = p.get(k) { o.as_ref().clone().into() }
        else { json!({"type": "constFloat", "c": c.get(k).and_then(|v| v.as_f64()).unwrap_or(d)}) }
    }
    let mut out = HashMap::new();
    out.insert("shader".to_string(), Message::object(EncodableValue::from(json!({
        "type": "musgraveTexture",
        "scale": ir_or_f(&payload, &config, "scale", 5.0),
        "detail": ir_or_f(&payload, &config, "detail", 2.0),
        "dimension": ir_or_f(&payload, &config, "dimension", 2.0),
    }))));
    Ok(out)
}

#[actor(ShaderWaveTextureActor, inports::<10>(scale, distortion), outports::<1>(shader), state(MemoryState))]
pub async fn shader_wave_texture_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();
    let wave_type = config.get("waveType").and_then(|v| v.as_str()).unwrap_or("bands");
    fn ir_or_f(p: &HashMap<String, Message>, c: &HashMap<String, Value>, k: &str, d: f64) -> Value {
        if let Some(Message::Object(o)) = p.get(k) { o.as_ref().clone().into() }
        else { json!({"type": "constFloat", "c": c.get(k).and_then(|v| v.as_f64()).unwrap_or(d)}) }
    }
    let mut out = HashMap::new();
    out.insert("shader".to_string(), Message::object(EncodableValue::from(json!({
        "type": "waveTexture", "waveType": wave_type,
        "scale": ir_or_f(&payload, &config, "scale", 5.0),
        "distortion": ir_or_f(&payload, &config, "distortion", 0.0),
    }))));
    Ok(out)
}
