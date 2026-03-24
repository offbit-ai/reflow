//! Principled BSDF + Material Output shader nodes.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Principled BSDF — collects shader IR subtrees from inports, nests them.
#[actor(
    ShaderPrincipledBsdfActor,
    inports::<10>(base_color, metallic, roughness, normal, emission, emission_strength, ao, alpha),
    outports::<1>(shader),
    state(MemoryState)
)]
pub async fn shader_principled_bsdf_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    fn get_shader_ir(payload: &HashMap<String, Message>, config: &HashMap<String, Value>, port: &str, default: Value) -> Value {
        if let Some(Message::Object(obj)) = payload.get(port) {
            obj.as_ref().clone().into()
        } else {
            config.get(port).cloned().unwrap_or(default)
        }
    }

    let base_color = get_shader_ir(&payload, &config, "base_color", json!({"type": "constVec3", "c": [0.8, 0.8, 0.8]}));
    let metallic = get_shader_ir(&payload, &config, "metallic", json!({"type": "constFloat", "c": 0.0}));
    let roughness = get_shader_ir(&payload, &config, "roughness", json!({"type": "constFloat", "c": 0.5}));
    let normal = payload.get("normal").map(|m| {
        if let Message::Object(obj) = m { let v: Value = obj.as_ref().clone().into(); v } else { json!(null) }
    }).filter(|v| !v.is_null());
    let emission = get_shader_ir(&payload, &config, "emission", json!({"type": "constVec3", "c": [0.0, 0.0, 0.0]}));
    let emission_strength = get_shader_ir(&payload, &config, "emission_strength", json!({"type": "constFloat", "c": 0.0}));
    let ao = payload.get("ao").map(|m| {
        if let Message::Object(obj) = m { let v: Value = obj.as_ref().clone().into(); v } else { json!(null) }
    }).filter(|v| !v.is_null());
    let alpha = get_shader_ir(&payload, &config, "alpha", json!({"type": "constFloat", "c": 1.0}));

    let ior_val = get_shader_ir(&payload, &config, "ior", json!({"type": "constFloat", "c": 1.5}));

    let mut bsdf = json!({
        "type": "principledBsdf",
        "base_color": base_color,
        "metallic": metallic,
        "roughness": roughness,
        "emission": emission,
        "emission_strength": emission_strength,
        "alpha": alpha,
        "ior": ior_val,
    });
    if let Some(n) = normal {
        bsdf["normal"] = n;
    }
    if let Some(a) = ao {
        bsdf["ao"] = a;
    }

    let mut out = HashMap::new();
    out.insert("shader".to_string(), Message::object(EncodableValue::from(bsdf)));
    Ok(out)
}

/// Material Output — wraps the surface shader IR in a MaterialOutput node.
#[actor(
    ShaderMaterialOutputActor,
    inports::<10>(surface),
    outports::<1>(shader),
    state(MemoryState)
)]
pub async fn shader_material_output_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
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
    out.insert("shader".to_string(), Message::object(EncodableValue::from(material_output)));
    Ok(out)
}
