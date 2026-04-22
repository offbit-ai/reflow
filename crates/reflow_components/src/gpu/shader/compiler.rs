//! Shader Compiler — receives ShaderNode IR tree and generates WGSL material.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_shader::ir::ShaderNode;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    ShaderCompilerActor,
    inports::<10>(shader),
    outports::<1>(material, wgsl, shade, error),
    state(MemoryState)
)]
pub async fn shader_compiler_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let ir_value = match payload.get("shader") {
        Some(Message::Object(obj)) => {
            let v: Value = obj.as_ref().clone().into();
            v
        }
        _ => return Ok(HashMap::new()),
    };

    // Deserialize IR tree
    let node: ShaderNode = match serde_json::from_value(ir_value.clone()) {
        Ok(n) => n,
        Err(e) => {
            let mut out = HashMap::new();
            out.insert(
                "error".to_string(),
                Message::Error(format!("Shader IR parse error: {}", e).into()),
            );
            return Ok(out);
        }
    };

    // Compile to WGSL
    let compiled = reflow_shader::compile(&node);

    // Output compiled material as JSON (scene render reads this)
    let mat_json = json!({
        "vertexWgsl": compiled.vertex_wgsl,
        "fragmentWgsl": compiled.fragment_wgsl,
        "vertexStride": compiled.vertex_stride,
        "textureSlots": compiled.texture_slots,
        "pipelineHash": compiled.pipeline_hash,
    });

    let mut out = HashMap::new();
    out.insert(
        "material".to_string(),
        Message::object(EncodableValue::from(mat_json)),
    );
    out.insert(
        "wgsl".to_string(),
        Message::String(Arc::new(compiled.fragment_wgsl)),
    );

    // Also compile SDF-compatible shade function (for SDF renderer integration)
    let shade_name = config
        .get("shadeName")
        .and_then(|v| v.as_str())
        .unwrap_or("shade");
    let sdf_shade = reflow_shader::codegen::compile_sdf_shade_named(&node, shade_name);
    out.insert("shade".to_string(), Message::String(Arc::new(sdf_shade)));

    Ok(out)
}
