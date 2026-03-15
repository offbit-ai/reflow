//! Reads a file from the local filesystem into Message::Bytes.
//!
//! Config: path (required)

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    FileLoadActor,
    inports::<1>(),
    outports::<50>(output, metadata, error),
    state(MemoryState)
)]
pub async fn file_load_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let path = config
        .get("path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let path = match path {
        Some(p) => p,
        None => return Ok(error_output("Missing 'path' in config")),
    };

    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let size = bytes.len();

            // Guess MIME type from extension
            let mime = match std::path::Path::new(&path)
                .extension()
                .and_then(|e| e.to_str())
            {
                Some("png") => "image/png",
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("webp") => "image/webp",
                Some("gif") => "image/gif",
                Some("wav") => "audio/wav",
                Some("mp3") => "audio/mpeg",
                Some("json") => "application/json",
                Some("txt") => "text/plain",
                _ => "application/octet-stream",
            };

            let mut results = HashMap::new();
            results.insert("output".to_string(), Message::bytes(bytes));
            results.insert(
                "metadata".to_string(),
                Message::object(EncodableValue::from(json!({
                    "path": path,
                    "size": size,
                    "mimeType": mime,
                }))),
            );
            Ok(results)
        }
        Err(e) => Ok(error_output(&format!("File read error: {}", e))),
    }
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
