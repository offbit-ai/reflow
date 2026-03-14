//! WSOLA (Waveform Similarity Overlap-Add) time stretching.
//!
//! Changes duration without changing pitch. Stretches or compresses
//! the audio by a configurable ratio.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{
    stream::{spawn_stream_task, StreamFrame},
    ActorContext,
};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    TimeStretchActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn time_stretch_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    // Stretch ratio: 2.0 = double duration (half speed), 0.5 = half duration
    let ratio = config
        .get("ratio")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;

    let window_size = config
        .get("windowSize")
        .and_then(|v| v.as_u64())
        .unwrap_or(1024) as usize;

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
        None,
        None,
    );

    spawn_stream_task(async move {
        // Collect all audio first (WSOLA needs lookahead)
        let mut stream = input_rx.into_stream();
        let mut all_samples: Vec<f32> = Vec::new();
        let mut begin_frame = None;

        while let Some(frame) = stream.next().await {
            match frame {
                StreamFrame::Begin { .. } => {
                    begin_frame = Some(frame);
                }
                StreamFrame::Data(data) => {
                    let samples: Vec<f32> = data
                        .chunks_exact(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect();
                    all_samples.extend_from_slice(&samples);
                }
                StreamFrame::End | StreamFrame::Error(_) => break,
            }
        }

        if let Some(bf) = begin_frame {
            let _ = tx.send_async(bf).await;
        }

        if all_samples.is_empty() || (ratio - 1.0).abs() < 0.001 {
            // No stretch needed
            let bytes: Vec<u8> =
                all_samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            let _ = tx
                .send_async(StreamFrame::Data(Arc::new(bytes)))
                .await;
            let _ = tx.send_async(StreamFrame::End).await;
            return;
        }

        // WSOLA
        let hop_a = window_size / 4; // analysis hop
        let hop_s = (hop_a as f32 * ratio) as usize; // synthesis hop
        let search_range = hop_a / 2;

        let hann: Vec<f32> = (0..window_size)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / window_size as f32).cos())
            })
            .collect();

        let mut output = vec![0.0f32; (all_samples.len() as f32 * ratio) as usize + window_size];
        let mut out_pos: usize = 0;
        let mut in_pos: usize = 0;

        while in_pos + window_size <= all_samples.len() {
            // Find best match in search range (maximize cross-correlation)
            let best_offset = if out_pos > 0 && in_pos + window_size + search_range <= all_samples.len() {
                let mut best = 0i32;
                let mut best_corr = f32::NEG_INFINITY;
                let range = search_range.min(all_samples.len() - in_pos - window_size);
                for offset in -(range as i32)..=(range as i32) {
                    let pos = (in_pos as i32 + offset) as usize;
                    if pos + window_size > all_samples.len() {
                        continue;
                    }
                    let mut corr: f32 = 0.0;
                    for i in 0..window_size.min(64) {
                        // Quick correlation on first 64 samples
                        if out_pos + i < output.len() {
                            corr += output[out_pos + i] * all_samples[pos + i];
                        }
                    }
                    if corr > best_corr {
                        best_corr = corr;
                        best = offset;
                    }
                }
                best
            } else {
                0
            };

            let read_pos = ((in_pos as i32 + best_offset).max(0) as usize)
                .min(all_samples.len().saturating_sub(window_size));

            // Overlap-add with Hann window
            for i in 0..window_size {
                if out_pos + i < output.len() && read_pos + i < all_samples.len() {
                    output[out_pos + i] += all_samples[read_pos + i] * hann[i];
                }
            }

            in_pos += hop_a;
            out_pos += hop_s;
        }

        // Trim trailing zeros
        let end = output.iter().rposition(|&s| s.abs() > 1e-10).unwrap_or(0) + 1;
        let output = &output[..end];

        // Emit in chunks
        let chunk_size = 4096;
        for chunk in output.chunks(chunk_size) {
            let bytes: Vec<u8> = chunk.iter().flat_map(|s| s.to_le_bytes()).collect();
            if tx
                .send_async(StreamFrame::Data(Arc::new(bytes)))
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
