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
