//! Converts an RGBA image stream to grayscale (Gray8).
//!
//! Each Data frame is a row of RGBA pixels (4 bytes/pixel). Output is
//! Gray8 (1 byte/pixel). Uses SIMD-accelerated path when available.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{
    stream::{spawn_stream_task, stream_transform_with_begin},
    ActorContext,
};
use std::collections::HashMap;

#[cfg(feature = "av-core")]
use reflow_pixel::color::row_rgba_to_gray;

#[actor(
    GrayscaleFilterActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn grayscale_filter_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let input_rx = match context.take_stream_receiver("stream") {
        Some(rx) => rx,
        None => return Ok(error_output("No StreamHandle on stream port")),
    };

    let payload = context.get_payload();
    let input_handle = match payload.get("stream") {
        Some(Message::StreamHandle(h)) => h,
        _ => return Ok(error_output("Expected StreamHandle message")),
    };

    // Output size is 1/4 of RGBA input
    let out_hint = input_handle.size_hint.map(|s| s / 4);

    let (tx, handle) =
        context.create_stream("stream", Some("image/raw-gray".to_string()), out_hint, None);

    spawn_stream_task(async move {
        stream_transform_with_begin(
            input_rx,
            tx,
            |_ct, _sh, metadata| {
                // Update content type and format in metadata
                let mut meta = metadata.unwrap_or(serde_json::json!({}));
                if let Some(obj) = meta.as_object_mut() {
                    obj.insert("format".to_string(), serde_json::json!("Gray8"));
                }
                (Some("image/raw-gray".to_string()), out_hint, Some(meta))
            },
            #[cfg(feature = "av-core")]
            |data: &[u8]| {
                let pixel_count = data.len() / 4;
                let mut out = vec![0u8; pixel_count];
                row_rgba_to_gray(data, &mut out);
                out
            },
            #[cfg(not(feature = "av-core"))]
            |data: &[u8]| {
                // Fallback: simple luminance
                data.chunks_exact(4)
                    .map(|px| {
                        ((px[0] as u32 * 77 + px[1] as u32 * 150 + px[2] as u32 * 29) >> 8) as u8
                    })
                    .collect()
            },
        )
        .await;
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
