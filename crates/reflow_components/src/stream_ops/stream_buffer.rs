//! Accumulates stream data frames into larger batches before forwarding.
//!
//! Useful when downstream actors perform better with larger chunks
//! (e.g., FFT needing a fixed window size, or network sends benefiting
//! from fewer larger packets).

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use futures::StreamExt;
use reflow_actor::{
    stream::{spawn_stream_task, StreamFrame},
    ActorContext,
};
use std::collections::HashMap;
use std::sync::Arc;

const DEFAULT_BUFFER_BYTES: usize = 65536;

#[actor(
    StreamBufferActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn stream_buffer_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let buffer_bytes = config
        .get("bufferBytes")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_BUFFER_BYTES);

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
        let mut accum = Vec::with_capacity(buffer_bytes);

        while let Some(frame) = stream.next().await {
            match frame {
                StreamFrame::Data(data) => {
                    accum.extend_from_slice(&data);
                    if accum.len() >= buffer_bytes {
                        let chunk = std::mem::replace(&mut accum, Vec::with_capacity(buffer_bytes));
                        if tx
                            .send_async(StreamFrame::Data(Arc::new(chunk)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
                StreamFrame::End => {
                    if !accum.is_empty() {
                        let _ = tx.send_async(StreamFrame::Data(Arc::new(accum))).await;
                    }
                    let _ = tx.send_async(StreamFrame::End).await;
                    break;
                }
                other => {
                    let is_terminal = other.is_terminal();
                    if tx.send_async(other).await.is_err() || is_terminal {
                        break;
                    }
                }
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
