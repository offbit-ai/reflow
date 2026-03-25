mod compiler;
mod effects;
mod inputs;
mod math;
mod principled;
mod textures;

use crate::Message;
use reflow_actor::ActorContext;
use serde_json::Value;
use std::collections::HashMap;

pub(super) fn update_shader_input_cache(
    ctx: &ActorContext,
    ports: &[&str],
) -> HashMap<String, Value> {
    for port in ports {
        if let Some(Message::Object(obj)) = ctx.get_payload().get(*port) {
            let value: Value = obj.as_ref().clone().into();
            ctx.pool_upsert("_shader_inputs", port, value);
        }
    }

    ctx.get_pool("_shader_inputs").into_iter().collect()
}

pub(super) fn cached_shader_input(cache: &HashMap<String, Value>, port: &str) -> Option<Value> {
    cache.get(port).cloned()
}

pub(super) fn connected_shader_inputs_ready(
    ctx: &ActorContext,
    cache: &HashMap<String, Value>,
    ports: &[&str],
) -> bool {
    ports.iter().all(|port| {
        let connected = ctx
            .config
            .inport_connection_counts
            .get(*port)
            .copied()
            .unwrap_or(0);
        connected == 0 || cache.contains_key(*port)
    })
}

pub use compiler::ShaderCompilerActor;
pub use effects::{
    ShaderBumpMapActor, ShaderClampActor, ShaderCombineXYZActor, ShaderMapRangeActor,
    ShaderMappingActor, ShaderNormalMapActor, ShaderSeparateXYZActor,
};
pub use inputs::{
    ShaderConstColorActor, ShaderConstFloatActor, ShaderNormalInputActor, ShaderPositionInputActor,
    ShaderTexCoordActor, ShaderTimeInputActor, ShaderVertexColorActor,
};
pub use math::{ShaderColorMixActor, ShaderColorRampActor, ShaderFresnelActor, ShaderMathActor};
pub use principled::{ShaderMaterialOutputActor, ShaderPrincipledBsdfActor};
pub use textures::{
    ShaderBrickTextureActor, ShaderCheckerTextureActor, ShaderGradientTextureActor,
    ShaderImageTextureActor, ShaderMusgraveTextureActor, ShaderNoiseTextureActor,
    ShaderVoronoiTextureActor, ShaderWaveTextureActor,
};
