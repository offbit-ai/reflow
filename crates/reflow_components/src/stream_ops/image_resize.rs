//! Streaming image resize using bilinear interpolation.
//!
//! Collects the full image from the stream (row-by-row), resizes it,
//! then re-emits as a new stream with updated dimensions.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use futures::StreamExt;
use reflow_actor::{
    stream::{spawn_stream_task, StreamFrame},
    ActorContext,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    ImageResizeActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn image_resize_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let target_width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let target_height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let channels = config.get("channels").and_then(|v| v.as_u64()).unwrap_or(4) as usize; // RGBA default

    let input_rx = match context.take_stream_receiver("stream") {
        Some(rx) => rx,
        None => return Ok(error_output("No StreamHandle on stream port")),
    };

    let payload = context.get_payload();
    let input_handle = match payload.get("stream") {
        Some(Message::StreamHandle(h)) => h,
        _ => return Ok(error_output("Expected StreamHandle message")),
    };

    let (tx, handle) = context.create_stream(
        "stream",
        input_handle.content_type.clone(),
        None, // size changes
        None,
    );

    spawn_stream_task(async move {
        let mut stream = input_rx.into_stream();
        let mut src_width: usize = 0;
        let mut src_height: usize = 0;
        let mut image_data: Vec<u8> = Vec::new();
        let mut content_type = None;
        let mut orig_meta = None;

        // Collect all frames
        while let Some(frame) = stream.next().await {
            match frame {
                StreamFrame::Begin {
                    content_type: ct,
                    metadata,
                    ..
                } => {
                    content_type = ct;
                    if let Some(ref m) = metadata {
                        src_width = m.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        src_height = m.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    }
                    orig_meta = metadata;
                }
                StreamFrame::Data(data) => {
                    image_data.extend_from_slice(&data);
                }
                StreamFrame::End => break,
                StreamFrame::Error(e) => {
                    let _ = tx.send_async(StreamFrame::Error(e)).await;
                    return;
                }
            }
        }

        // Infer dimensions if not in metadata
        if src_width == 0 && src_height == 0 && channels > 0 {
            let total_pixels = image_data.len() / channels;
            // Assume square if no info
            let side = (total_pixels as f64).sqrt() as usize;
            src_width = side;
            src_height = total_pixels / side;
        }

        if src_width == 0 || src_height == 0 {
            let _ = tx
                .send_async(StreamFrame::Error(
                    "Cannot determine source dimensions".to_string(),
                ))
                .await;
            return;
        }

        let out_w = if target_width > 0 {
            target_width
        } else {
            src_width
        };
        let out_h = if target_height > 0 {
            target_height
        } else {
            src_height
        };

        // Resize
        #[cfg(feature = "av-core")]
        let resized = {
            let fmt = match channels {
                1 => reflow_pixel::format::PixelFormat::Gray8,
                2 => reflow_pixel::format::PixelFormat::GrayAlpha8,
                3 => reflow_pixel::format::PixelFormat::Rgb8,
                _ => reflow_pixel::format::PixelFormat::Rgba8,
            };
            reflow_pixel::resize::resize(&image_data, src_width, src_height, out_w, out_h, fmt)
        };

        #[cfg(not(feature = "av-core"))]
        let resized = {
            // Nearest-neighbor fallback
            let mut out = vec![0u8; out_w * out_h * channels];
            for y in 0..out_h {
                let sy = y * src_height / out_h;
                for x in 0..out_w {
                    let sx = x * src_width / out_w;
                    let src_off = (sy * src_width + sx) * channels;
                    let dst_off = (y * out_w + x) * channels;
                    if src_off + channels <= image_data.len() {
                        out[dst_off..dst_off + channels]
                            .copy_from_slice(&image_data[src_off..src_off + channels]);
                    }
                }
            }
            out
        };

        // Emit resized as new stream
        let new_size = resized.len() as u64;
        let mut meta = orig_meta.unwrap_or(json!({}));
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("width".to_string(), json!(out_w));
            obj.insert("height".to_string(), json!(out_h));
        }

        let _ = tx
            .send_async(StreamFrame::Begin {
                content_type,
                size_hint: Some(new_size),
                metadata: Some(meta),
            })
            .await;

        // Emit row by row
        let row_bytes = out_w * channels;
        for row in resized.chunks(row_bytes) {
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
