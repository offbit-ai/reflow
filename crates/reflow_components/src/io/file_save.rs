//! Saves Message::Bytes to a file on the local filesystem.
//!
//! Config: path (required), createDirs (auto-create parent directories)

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    FileSaveActor,
    inports::<100>(input),
    outports::<50>(path, metadata, error),
    state(MemoryState)
)]
pub async fn file_save_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();

    let path = config
        .get("path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let path = match path {
        Some(p) => p,
        None => return Ok(error_output("Missing 'path' in config")),
    };

    let create_dirs = config
        .get("createDirs")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let bytes = match payload.get("input") {
        Some(Message::Bytes(data)) => Arc::clone(data),
        Some(Message::String(s)) => Arc::new(s.as_bytes().to_vec()),
        _ => return Ok(error_output("Expected Bytes or String on input port")),
    };

    // Create parent directories if needed
    if create_dirs {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return Ok(error_output(&format!("Failed to create directories: {}", e)));
            }
        }
    }

    // Write file
    match tokio::fs::write(&path, bytes.as_ref()).await {
        Ok(()) => {
            let size = bytes.len();
            let mut results = HashMap::new();
            results.insert("path".to_string(), Message::String(path.clone().into()));
            results.insert(
                "metadata".to_string(),
                Message::object(EncodableValue::from(json!({
                    "path": path,
                    "size": size,
                    "written": true,
                }))),
            );
            Ok(results)
        }
        Err(e) => Ok(error_output(&format!("File write error: {}", e))),
    }
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
