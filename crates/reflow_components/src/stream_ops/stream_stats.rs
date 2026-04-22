//! Passthrough stream that collects throughput and frame statistics.
//!
//! All frames pass through unmodified; stats are emitted on the `stats`
//! outport after the stream terminates.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::{actor, actor_display};
use anyhow::{Error, Result};
use futures::StreamExt;
use reflow_actor::{
    message::EncodableValue,
    stream::{spawn_stream_task, StreamFrame},
    ActorContext,
};
use serde_json::json;
use std::collections::HashMap;

#[actor_display(
    actor = StreamStatsActor,
    id = "tpl_stream_stats",
    title = "Stream Stats",
    subtitle = "Measure throughput",
    category = "stream",
    subcategory = "plumbing",
    description = "Passthrough that measures total bytes, frame count, duration, and throughput without modifying the stream.",
    icon = "bar-chart-2",
    variant = "blue-500",
    inputs(stream = "stream"),
    outputs(stream = "stream", stats = "object", error = "string"),
    display(
        element = "reflow-stats",
        source = concat!(
            "if(!globalThis.ReflowUI){",
            include_str!("../../../../display_components/reflow-ui.js"),
            "}\n",
            include_str!("../../../../display_components/stats.js")
        ),
        shadow = true
    )
)]
#[actor(
    StreamStatsActor,
    inports::<100>(stream),
    outports::<50>(stream, stats, error),
    state(MemoryState)
)]
pub async fn stream_stats_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let input_rx = match context.take_stream_receiver("stream") {
        Some(rx) => rx,
        None => {
            return Ok(error_output("No StreamHandle on stream port"));
        }
    };

    let payload = context.get_payload();
    let input_handle = match payload.get("stream") {
        Some(Message::StreamHandle(h)) => h,
        _ => {
            return Ok(error_output("Expected StreamHandle message"));
        }
    };

    let content_type_for_stats = input_handle.content_type.clone();

    let (tx, handle) = context.create_stream(
        "stream",
        input_handle.content_type.clone(),
        input_handle.size_hint,
        None,
    );

    // Collect stats via a oneshot channel from the forwarding task
    let (stats_tx, stats_rx) = flume::bounded::<serde_json::Value>(1);

    let ct = content_type_for_stats;
    spawn_stream_task(async move {
        let mut stream = input_rx.into_stream();
        let mut total_bytes: u64 = 0;
        let mut data_frames: u64 = 0;
        let mut size_hint: Option<u64> = None;
        let start = std::time::Instant::now();

        while let Some(frame) = stream.next().await {
            let is_terminal = frame.is_terminal();

            match &frame {
                StreamFrame::Begin { size_hint: sh, .. } => {
                    size_hint = *sh;
                }
                StreamFrame::Data(data) => {
                    total_bytes += data.len() as u64;
                    data_frames += 1;
                }
                _ => {}
            }

            if tx.send_async(frame).await.is_err() {
                break;
            }
            if is_terminal {
                break;
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let throughput_bps = if duration_ms > 0 {
            (total_bytes * 1000) / duration_ms
        } else {
            0
        };
        let avg_frame_bytes = if data_frames > 0 {
            total_bytes / data_frames
        } else {
            0
        };

        let _ = stats_tx.send(json!({
            "totalBytes": total_bytes,
            "dataFrames": data_frames,
            "durationMs": duration_ms,
            "throughputBytesPerSec": throughput_bps,
            "avgFrameBytes": avg_frame_bytes,
            "sizeHint": size_hint,
            "contentType": ct,
        }));
    });

    // Wait for stats (stream must finish first)
    let stats_value = stats_rx.recv_async().await.unwrap_or(json!({}));

    let mut results = HashMap::new();
    results.insert("stream".to_string(), Message::stream_handle(handle));
    results.insert(
        "stats".to_string(),
        Message::object(EncodableValue::from(stats_value)),
    );
    Ok(results)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
