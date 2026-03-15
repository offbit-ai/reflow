//! Image stream display actor — consumes an image stream and emits metadata
//! for Zeal's visual display node.
//!
//! This actor is the Reflow-side counterpart of a Zeal display node. It:
//! 1. Receives a `StreamHandle` on its `stream` inport
//! 2. Reads the `Begin` frame to extract content type, dimensions, and metadata
//! 3. Drains remaining frames (the EventBridge observer tap forwards binary
//!    data to Zeal independently — this actor just keeps backpressure flowing)
//! 4. Outputs image metadata on `metadata` for downstream nodes

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, stream::StreamFrame, ActorContext};
use serde_json::json;
use std::collections::HashMap;

const DEFAULT_IMAGE_CONTENT_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/raw-rgba",
    "image/raw-gray",
];

/// Image Stream Display Actor — compatible with `tpl_image_stream_display`
///
/// Consumes an image stream handle and drains it, allowing the observer
/// tap in the EventBridge to forward binary frames to Zeal for live
/// rendering. Outputs metadata about the stream for downstream use.
#[actor(
    ImageStreamDisplayActor,
    inports::<100>(stream),
    outports::<50>(metadata, error),
    state(MemoryState)
)]
pub async fn image_stream_display_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let display_mode = config
        .get("displayMode")
        .and_then(|v| v.as_str())
        .unwrap_or("contain");

    // Take the stream receiver
    let stream_rx = context
        .take_stream_receiver("stream")
        .ok_or_else(|| anyhow::anyhow!("No stream handle on 'stream' port"))?;

    // Read Begin frame for metadata
    let first = stream_rx
        .recv_async()
        .await
        .map_err(|_| anyhow::anyhow!("Stream closed before Begin frame"))?;

    let (content_type, size_hint, stream_meta) = match first {
        StreamFrame::Begin {
            content_type,
            size_hint,
            metadata,
        } => (content_type, size_hint, metadata),
        StreamFrame::Error(e) => return Ok(error_output(format!("Stream error: {}", e))),
        _ => return Ok(error_output("Expected Begin frame, got data".into())),
    };

    let ct = content_type.as_deref().unwrap_or("application/octet-stream");

    // Validate image content type
    let accepted: Vec<String> = config
        .get("acceptedFormats")
        .and_then(|v| v.as_str())
        .map(|s| s.split(',').map(|f| f.trim().to_string()).collect())
        .unwrap_or_else(|| {
            DEFAULT_IMAGE_CONTENT_TYPES
                .iter()
                .map(|s| s.to_string())
                .collect()
        });

    if !accepted.iter().any(|f| ct.starts_with(f.as_str())) {
        return Ok(error_output(format!(
            "Unsupported image stream format: {}",
            ct
        )));
    }

    // Extract dimensions from stream metadata if available
    let width = stream_meta
        .as_ref()
        .and_then(|m| m.get("width"))
        .and_then(|v| v.as_u64());
    let height = stream_meta
        .as_ref()
        .and_then(|m| m.get("height"))
        .and_then(|v| v.as_u64());
    let format = stream_meta
        .as_ref()
        .and_then(|m| m.get("format"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Drain remaining frames to maintain backpressure on the producer.
    // The actual binary data is forwarded to Zeal by the observer tap.
    let mut total_bytes: u64 = 0;
    let mut chunk_count: u64 = 0;
    loop {
        match stream_rx.recv_async().await {
            Ok(StreamFrame::Data(data)) => {
                total_bytes += data.len() as u64;
                chunk_count += 1;
            }
            Ok(StreamFrame::End) => break,
            Ok(StreamFrame::Error(e)) => {
                return Ok(error_output(format!("Stream error during transfer: {}", e)));
            }
            Ok(StreamFrame::Begin { .. }) => {} // ignore duplicate Begin
            Err(_) => break,                    // channel closed
        }
    }

    let metadata = json!({
        "contentType": ct,
        "sizeHint": size_hint,
        "totalBytes": total_bytes,
        "chunks": chunk_count,
        "width": width,
        "height": height,
        "format": format,
        "displayMode": display_mode,
        "streamMetadata": stream_meta,
    });

    let mut output = HashMap::new();
    output.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(metadata)),
    );
    Ok(output)
}

fn error_output(msg: String) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.into()));
    out
}
