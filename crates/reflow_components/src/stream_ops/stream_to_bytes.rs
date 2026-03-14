//! Collects an entire stream into a `Message::Bytes` blob.
//!
//! The inverse of `BytesToStreamActor`. Useful for saving streamed data
//! to a file, passing to an HTTP endpoint, or any actor that needs the
//! full buffer.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, stream::stream_collect, ActorContext};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    StreamToBytesActor,
    inports::<100>(stream),
    outports::<50>(output, metadata, error),
    state(MemoryState)
)]
pub async fn stream_to_bytes_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let rx = match context.take_stream_receiver("stream") {
        Some(rx) => rx,
        None => {
            return Ok(error_output("No StreamHandle on stream port"));
        }
    };

    match stream_collect(rx).await {
        Ok((content_type, metadata, bytes)) => {
            let len = bytes.len();
            let mut results = HashMap::new();
            results.insert("output".to_string(), Message::bytes(bytes));
            results.insert(
                "metadata".to_string(),
                Message::object(EncodableValue::from(json!({
                    "contentType": content_type,
                    "size": len,
                    "streamMetadata": metadata,
                }))),
            );
            Ok(results)
        }
        Err(e) => Ok(error_output(&format!("Stream error: {}", e))),
    }
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
