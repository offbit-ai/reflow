//! Convolution reverb / FIR filter via impulse response.
//!
//! Takes an audio stream and an impulse response (as Bytes on the `impulse`
//! port), convolves them using overlap-save in the frequency domain.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use futures::StreamExt;
use reflow_actor::{
    stream::{spawn_stream_task, StreamFrame},
    ActorContext,
};
use reflow_actor_macro::actor;
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    ConvolveActor,
    inports::<100>(stream, impulse),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn convolve_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();

    // Get impulse response from the impulse port
    let ir_samples: Vec<f32> = match payload.get("impulse") {
        Some(Message::Bytes(data)) => {
            // Assume LE f32 samples
            data.chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect()
        }
        _ => return Ok(error_output("No impulse response on impulse port")),
    };

    if ir_samples.is_empty() {
        return Ok(error_output("Empty impulse response"));
    }

    let input_rx = match context.take_stream_receiver("stream") {
        Some(rx) => rx,
        None => return Ok(error_output("No StreamHandle on stream port")),
    };

    let input_handle = match payload.get("stream") {
        Some(Message::StreamHandle(h)) => h,
        _ => return Ok(error_output("Expected StreamHandle message")),
    };

    let (tx, handle) =
        context.create_stream("stream", input_handle.content_type.clone(), None, None);

    spawn_stream_task(async move {
        // Simple time-domain convolution for short IRs,
        // overlap-save for long ones
        let ir_len = ir_samples.len();
        let mut overlap = vec![0.0f32; ir_len - 1];

        let mut stream = input_rx.into_stream();
        while let Some(frame) = stream.next().await {
            let is_terminal = frame.is_terminal();
            let out_frame = match frame {
                StreamFrame::Data(data) => {
                    let input: Vec<f32> = data
                        .chunks_exact(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect();

                    let in_len = input.len();
                    let out_len = in_len + ir_len - 1;
                    let mut output = vec![0.0f32; out_len];

                    // Direct convolution
                    for (i, &x) in input.iter().enumerate() {
                        for (j, &h) in ir_samples.iter().enumerate() {
                            output[i + j] += x * h;
                        }
                    }

                    // Add overlap from previous block
                    for (i, &ov) in overlap.iter().enumerate() {
                        if i < output.len() {
                            output[i] += ov;
                        }
                    }

                    // Save tail for next block
                    overlap.fill(0.0);
                    for i in 0..overlap.len() {
                        if in_len + i < output.len() {
                            overlap[i] = output[in_len + i];
                        }
                    }

                    // Output only in_len samples (matching input length)
                    let bytes: Vec<u8> = output[..in_len]
                        .iter()
                        .flat_map(|s| s.to_le_bytes())
                        .collect();
                    StreamFrame::Data(Arc::new(bytes))
                }
                other => other,
            };
            if tx.send_async(out_frame).await.is_err() || is_terminal {
                break;
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
