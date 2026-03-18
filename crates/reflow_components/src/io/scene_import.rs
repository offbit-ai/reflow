//! Auto-detecting scene import actor.
//!
//! Accepts raw file bytes, detects the format, and outputs all
//! extractable components: mesh, skeleton, animation, skin weights.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::ActorContext;
use std::collections::HashMap;

#[actor(
    SceneImportActor,
    inports::<10>(file_data),
    outports::<1>(mesh, skeleton, inverse_bind_matrices, clip, skin, skin_descriptor, metadata, error),
    state(MemoryState)
)]
pub async fn scene_import_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let data = match payload.get("file_data") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => return Ok(error_output("Expected Bytes on file_data port")),
    };

    let format = super::mesh_import::detect_format(&data);

    match format.as_str() {
        "glb" | "gltf" => super::gltf_import::import_gltf(&data, &config),
        "stl" => super::stl_import::parse_stl(&data, false),

        "obj" => super::obj_import::parse_obj(&data),
        _ => Ok(error_output(
            "Unknown or unsupported format for scene import",
        )),
    }
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
