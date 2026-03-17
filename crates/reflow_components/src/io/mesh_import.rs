//! Auto-detecting mesh import actor.
//!
//! Accepts raw file bytes, detects the format (glTF/GLB, OBJ, STL),
//! and outputs mesh data in Reflow's internal format.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    MeshImportActor,
    inports::<10>(file_data),
    outports::<1>(mesh, metadata, error),
    state(MemoryState)
)]
pub async fn mesh_import_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let data = match payload.get("file_data") {
        Some(Message::Bytes(b)) => b.clone(),
        Some(Message::String(s)) => std::sync::Arc::new(s.as_bytes().to_vec()),
        _ => return Ok(error_output("Expected Bytes on file_data port")),
    };

    let format = config
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("auto");

    let detected = if format == "auto" {
        detect_format(&data)
    } else {
        format.to_string()
    };

    match detected.as_str() {

        "glb" | "gltf" => {
            // Use glTF import, extract mesh only
            let full = super::gltf_import::import_gltf(&data, &config)?;
            let mut out = HashMap::new();
            if let Some(m) = full.get("mesh") {
                out.insert("mesh".to_string(), m.clone());
            }
            if let Some(m) = full.get("metadata") {
                out.insert("metadata".to_string(), m.clone());
            }
            if let Some(e) = full.get("error") {
                out.insert("error".to_string(), e.clone());
            }
            Ok(out)
        }
        "stl" => {
            super::stl_import::parse_stl(&data, false)
        }

        "obj" => {
            super::obj_import::parse_obj(&data)
        }
        _ => Ok(error_output(&format!("Unknown format: {}", detected))),
    }
}

pub(crate) fn detect_format(data: &[u8]) -> String {
    if data.len() >= 4 && &data[0..4] == b"glTF" {
        return "glb".to_string();
    }
    if data.len() >= 1 && data[0] == b'{' {
        if let Ok(s) = std::str::from_utf8(&data[..data.len().min(256)]) {
            if s.contains("\"asset\"") || s.contains("\"scenes\"") {
                return "gltf".to_string();
            }
        }
    }
    if let Ok(s) = std::str::from_utf8(&data[..data.len().min(128)]) {
        let trimmed = s.trim_start();
        if trimmed.starts_with("v ") || trimmed.starts_with("# ") || trimmed.starts_with("o ") {
            return "obj".to_string();
        }
    }
    if data.len() >= 84 {
        let tri_count =
            u32::from_le_bytes([data[80], data[81], data[82], data[83]]) as usize;
        let expected = 84 + tri_count * 50;
        if data.len() >= expected && tri_count > 0 && tri_count < 50_000_000 {
            return "stl".to_string();
        }
    }
    "unknown".to_string()
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
