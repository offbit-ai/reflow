//! Shader input nodes — source nodes that read from vertex data or constants.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

#[actor(ShaderTexCoordActor, inports::<1>(_trigger), outports::<1>(shader), state(MemoryState))]
pub async fn shader_tex_coord_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(json!({"type": "texCoord"}))),
    );
    Ok(out)
}

#[actor(ShaderPositionInputActor, inports::<1>(_trigger), outports::<1>(shader), state(MemoryState))]
pub async fn shader_position_input_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(json!({"type": "objectPosition"}))),
    );
    Ok(out)
}

#[actor(ShaderNormalInputActor, inports::<1>(_trigger), outports::<1>(shader), state(MemoryState))]
pub async fn shader_normal_input_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(json!({"type": "objectNormal"}))),
    );
    Ok(out)
}

#[actor(ShaderTimeInputActor, inports::<1>(_trigger), outports::<1>(shader), state(MemoryState))]
pub async fn shader_time_input_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(json!({"type": "time"}))),
    );
    Ok(out)
}

#[actor(ShaderVertexColorActor, inports::<1>(_trigger), outports::<1>(shader), state(MemoryState))]
pub async fn shader_vertex_color_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(json!({"type": "vertexColor"}))),
    );
    Ok(out)
}

#[actor(ShaderConstFloatActor, inports::<1>(_trigger), outports::<1>(shader), state(MemoryState))]
pub async fn shader_const_float_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let value = config.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(
            json!({"type": "constFloat", "c": value}),
        )),
    );
    Ok(out)
}

#[actor(ShaderConstColorActor, inports::<1>(_trigger), outports::<1>(shader), state(MemoryState))]
pub async fn shader_const_color_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let r = config.get("r").and_then(|v| v.as_f64()).unwrap_or(0.8);
    let g = config.get("g").and_then(|v| v.as_f64()).unwrap_or(0.8);
    let b = config.get("b").and_then(|v| v.as_f64()).unwrap_or(0.8);
    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(
            json!({"type": "constVec3", "c": [r, g, b]}),
        )),
    );
    Ok(out)
}
