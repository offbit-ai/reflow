//! Render frame collector — accumulates RGBA frames into a video stream.
//!
//! Receives one frame per invocation from SceneRenderActor. On the first
//! invocation, creates an output stream and sends Begin with video metadata.
//! Each subsequent frame is sent as StreamFrame::Data. On the final frame
//! (frame_number >= totalFrames), sends StreamFrame::End.
//!
//! Uses STREAM_REGISTRY.clone_sender() to persist the stream sender across
//! multiple actor invocations via the stream ID stored in MemoryState.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{
    message::EncodableValue,
    stream::{StreamFrame, StreamHandle, STREAM_REGISTRY},
    ActorContext, MemoryState,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    RenderFrameCollectorActor,
    inports::<100>(frame, frame_number, done),
    outports::<50>(stream, progress, error),
    state(MemoryState),
    await_inports(frame)
)]
pub async fn render_frame_collector_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let total_frames = config
        .get("totalFrames")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
    let height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
    let fps = config.get("fps").and_then(|v| v.as_u64()).unwrap_or(30) as u32;

    // frame guaranteed present via await_inports
    let frame_bytes = match payload.get("frame") {
        Some(Message::Bytes(b)) => b.clone(),
        _ => unreachable!("await_inports guarantees frame"),
    };

    let frame_number = match payload.get("frame_number") {
        Some(Message::Integer(i)) => *i as usize,
        _ => 0,
    };

    let is_done = match payload.get("done") {
        Some(Message::Boolean(b)) => *b,
        Some(Message::Integer(i)) => *i != 0,
        Some(Message::Float(f)) => *f != 0.0,
        Some(Message::Flow) => true,
        None => false,
        _ => false,
    };

    // Check if we already have a stream
    let state: HashMap<String, serde_json::Value> =
        ctx.get_pool("_collector").into_iter().collect();
    let existing_stream_id = state.get("stream_id").and_then(|v| v.as_u64());

    let mut results = HashMap::new();

    if let Some(stream_id) = existing_stream_id {
        // Subsequent frame — get sender from registry
        if let Some(tx) = STREAM_REGISTRY.clone_sender(stream_id) {
            let _ = tx.send(StreamFrame::Data(Arc::new(frame_bytes.to_vec())));

            let count = state
                .get("frame_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                + 1;
            ctx.pool_upsert("_collector", "frame_count", json!(count));

            // Check if done — allow 1 frame tolerance for pipeline warmup
            if is_done || (total_frames > 0 && count as usize + 1 >= total_frames) {
                let _ = tx.send(StreamFrame::End);
            }

            results.insert(
                "progress".to_string(),
                Message::object(EncodableValue::from(json!({
                    "frame": count,
                    "totalFrames": total_frames,
                }))),
            );
        }
    } else {
        // First frame — create the stream
        let (tx, handle) = ctx.create_stream(
            "stream",
            Some("video/raw-rgba".to_string()),
            Some((total_frames as u64) * (width as u64) * (height as u64) * 4),
            Some(total_frames.max(64)),
        );

        // Store stream ID for subsequent invocations
        ctx.pool_upsert("_collector", "stream_id", json!(handle.stream_id));
        ctx.pool_upsert("_collector", "frame_count", json!(1u64));

        // Send Begin with video metadata
        let _ = tx.send(StreamFrame::Begin {
            content_type: Some("video/raw-rgba".to_string()),
            size_hint: None,
            metadata: Some(json!({
                "width": width,
                "height": height,
                "fps": fps,
                "totalFrames": total_frames,
            })),
        });

        // Send first frame
        let _ = tx.send(StreamFrame::Data(Arc::new(frame_bytes.to_vec())));

        // Check if only 1 frame total
        if is_done || (total_frames == 1) {
            let _ = tx.send(StreamFrame::End);
        }

        results.insert("stream".to_string(), Message::stream_handle(handle));
    }

    Ok(results)
}
