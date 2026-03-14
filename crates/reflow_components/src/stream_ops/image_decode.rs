//! Decodes PNG/JPEG/WebP/GIF bytes into a raw RGBA stream.
//!
//! Takes Message::Bytes (encoded image) on `input` and produces a
//! StreamHandle on `stream` with rows of RGBA pixels.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use image::GenericImageView;
use reflow_actor::{
    stream::{spawn_stream_task, StreamFrame},
    ActorContext,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    ImageDecodeActor,
    inports::<100>(input),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn image_decode_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();

    let bytes = match payload.get("input") {
        Some(Message::Bytes(data)) => Arc::clone(data),
        _ => return Ok(error_output("Expected Bytes on input port")),
    };

    // Decode image
    let img = match image::load_from_memory(&bytes) {
        Ok(img) => img,
        Err(e) => return Ok(error_output(&format!("Image decode error: {}", e))),
    };

    let (width, height) = img.dimensions();
    let rgba = img.to_rgba8();
    let raw_pixels = rgba.into_raw();
    let total_bytes = raw_pixels.len() as u64;

    // Create stream
    let (tx, handle) = context.create_stream(
        "stream",
        Some("image/raw-rgba".to_string()),
        Some(total_bytes),
        None,
    );

    spawn_stream_task(async move {
        // Begin frame with dimensions
        let _ = tx
            .send_async(StreamFrame::Begin {
                content_type: Some("image/raw-rgba".to_string()),
                size_hint: Some(total_bytes),
                metadata: Some(json!({
                    "width": width,
                    "height": height,
                    "format": "RGBA8",
                    "channels": 4,
                })),
            })
            .await;

        // Stream row by row
        let row_bytes = (width as usize) * 4;
        for row in raw_pixels.chunks(row_bytes) {
            if tx
                .send_async(StreamFrame::Data(Arc::new(row.to_vec())))
                .await
                .is_err()
            {
                return;
            }
        }

        let _ = tx.send_async(StreamFrame::End).await;
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
