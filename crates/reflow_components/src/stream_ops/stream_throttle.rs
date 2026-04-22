//! Rate-limits stream frame throughput.
//!
//! Inserts delays between Data frames to simulate real-time playback
//! speed or prevent overwhelming a slow consumer.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use futures::StreamExt;
use reflow_actor::{
    stream::{spawn_stream_task, StreamFrame},
    ActorContext,
};
use reflow_actor_macro::actor;
use std::collections::HashMap;

#[actor(
    StreamThrottleActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn stream_throttle_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let delay_ms = config.get("delayMs").and_then(|v| v.as_u64()).unwrap_or(10);

    let bytes_per_second = config.get("bytesPerSecond").and_then(|v| v.as_u64());

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

    let (tx, handle) = context.create_stream(
        "stream",
        input_handle.content_type.clone(),
        input_handle.size_hint,
        None,
    );

    spawn_stream_task(async move {
        let mut stream = input_rx.into_stream();

        while let Some(frame) = stream.next().await {
            let is_terminal = frame.is_terminal();

            let frame_delay = match (&frame, bytes_per_second) {
                (StreamFrame::Data(data), Some(bps)) if bps > 0 => {
                    std::time::Duration::from_millis((data.len() as u64 * 1000) / bps)
                }
                (StreamFrame::Data(_), _) => std::time::Duration::from_millis(delay_ms),
                _ => std::time::Duration::ZERO,
            };

            if tx.send_async(frame).await.is_err() {
                break;
            }

            if is_terminal {
                break;
            }

            if !frame_delay.is_zero() {
                #[cfg(not(target_arch = "wasm32"))]
                tokio::time::sleep(frame_delay).await;
            }
        }
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
