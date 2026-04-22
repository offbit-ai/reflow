//! Principled BSDF + Material Output shader nodes.

use super::{cached_shader_input, connected_shader_inputs_ready, update_shader_input_cache};
use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Principled BSDF — collects shader IR subtrees from inports, nests them.
#[actor(
    ShaderPrincipledBsdfActor,
    inports::<10>(base_color, metallic, roughness, normal, emission, emission_strength, ao, alpha, transmission, ior),
    outports::<1>(shader),
    state(MemoryState)
)]
pub async fn shader_principled_bsdf_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let cache = update_shader_input_cache(
        &ctx,
        &[
            "base_color",
            "metallic",
            "roughness",
            "normal",
            "emission",
            "emission_strength",
            "ao",
            "alpha",
            "transmission",
            "ior",
        ],
    );
    if !connected_shader_inputs_ready(
        &ctx,
        &cache,
        &[
            "base_color",
            "metallic",
            "roughness",
            "normal",
            "emission",
            "emission_strength",
            "ao",
            "alpha",
            "transmission",
            "ior",
        ],
    ) {
        return Ok(HashMap::new());
    }

    fn get_shader_ir(
        cache: &HashMap<String, Value>,
        config: &HashMap<String, Value>,
        port: &str,
        default: Value,
    ) -> Value {
        cached_shader_input(cache, port)
            .or_else(|| config.get(port).cloned())
            .unwrap_or(default)
    }

    let base_color = get_shader_ir(
        &cache,
        &config,
        "base_color",
        json!({"type": "constVec3", "c": [0.8, 0.8, 0.8]}),
    );
    let metallic = get_shader_ir(
        &cache,
        &config,
        "metallic",
        json!({"type": "constFloat", "c": 0.0}),
    );
    let roughness = get_shader_ir(
        &cache,
        &config,
        "roughness",
        json!({"type": "constFloat", "c": 0.5}),
    );
    let normal = cached_shader_input(&cache, "normal");
    let emission = get_shader_ir(
        &cache,
        &config,
        "emission",
        json!({"type": "constVec3", "c": [0.0, 0.0, 0.0]}),
    );
    let emission_strength = get_shader_ir(
        &cache,
        &config,
        "emission_strength",
        json!({"type": "constFloat", "c": 0.0}),
    );
    let ao = cached_shader_input(&cache, "ao");
    let alpha = get_shader_ir(
        &cache,
        &config,
        "alpha",
        json!({"type": "constFloat", "c": 1.0}),
    );
    let transmission = get_shader_ir(
        &cache,
        &config,
        "transmission",
        json!({"type": "constFloat", "c": 0.0}),
    );

    let ior_val = get_shader_ir(
        &cache,
        &config,
        "ior",
        json!({"type": "constFloat", "c": 1.5}),
    );

    let mut bsdf = json!({
        "type": "principledBsdf",
        "base_color": base_color,
        "metallic": metallic,
        "roughness": roughness,
        "emission": emission,
        "emission_strength": emission_strength,
        "alpha": alpha,
        "transmission": transmission,
        "ior": ior_val,
    });
    if let Some(n) = normal {
        bsdf["normal"] = n;
    }
    if let Some(a) = ao {
        bsdf["ao"] = a;
    }

    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(bsdf)),
    );
    Ok(out)
}

/// Material Output — wraps the surface shader IR in a MaterialOutput node.
#[actor(
    ShaderMaterialOutputActor,
    inports::<10>(surface),
    outports::<1>(shader),
    state(MemoryState)
)]
pub async fn shader_material_output_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();

    let surface = match payload.get("surface") {
        Some(Message::Object(obj)) => {
            let v: Value = obj.as_ref().clone().into();
            v
        }
        _ => return Ok(HashMap::new()),
    };

    let material_output = json!({
        "type": "materialOutput",
        "surface": surface,
    });

    let mut out = HashMap::new();
    out.insert(
        "shader".to_string(),
        Message::object(EncodableValue::from(material_output)),
    );
    Ok(out)
}
