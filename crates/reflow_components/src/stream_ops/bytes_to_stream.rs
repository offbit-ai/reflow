//! Converts a `Message::Bytes` blob into a chunked `StreamHandle`.
//!
//! Useful for bridging static data (file uploads, HTTP responses) into
//! the streaming pipeline so that downstream actors can process it
//! incrementally.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{
    stream::{spawn_stream_task, stream_from_bytes},
    ActorContext,
};
use std::collections::HashMap;
use std::sync::Arc;

const DEFAULT_CHUNK_SIZE: usize = 65536;

#[actor(
    BytesToStreamActor,
    inports::<100>(input),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn bytes_to_stream_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();

    let chunk_size = config
        .get("chunkSize")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_CHUNK_SIZE);

    let content_type = config
        .get("contentType")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let bytes = match payload.get("input") {
        Some(Message::Bytes(data)) => Arc::clone(data),
        Some(Message::String(s)) => Arc::new(s.as_bytes().to_vec()),
        Some(_) => {
            return Ok(error_output("Expected Bytes or String on input port"));
        }
        None => {
            return Ok(error_output("No data on input port"));
        }
    };

    let (tx, handle) = context.create_stream(
        "stream",
        content_type.clone(),
        Some(bytes.len() as u64),
        None,
    );

    let ct = content_type;
    spawn_stream_task(async move {
        let _ = stream_from_bytes(tx, &bytes, chunk_size, ct, None).await;
    });

    let mut results = HashMap::new();
    results.insert("stream".to_string(), Message::stream_handle(handle));
    Ok(results)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
