//! Peak normalization for audio streams.
//!
//! Two-pass approach: first pass scans for peak level (requires collecting
//! all data), second pass applies gain. For streaming, uses a single-pass
//! adaptive approach with a lookahead buffer.
//!
//! Data frames are raw PCM f32 samples (little-endian).

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
    AudioNormalizeActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn audio_normalize_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    // Target peak level in dB (0.0 = full scale, -1.0 = 1dB headroom)
    let target_db = config
        .get("targetDb")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);

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
        input_handle.size_hint,
        None,
    );

    spawn_stream_task(async move {
        // Two-pass: collect all data, find peak, apply gain
        let mut stream = input_rx.into_stream();
        let mut all_chunks: Vec<StreamFrame> = Vec::new();
        let mut peak: f32 = 0.0;
        let mut begin_frame = None;

        // Pass 1: collect and find peak
        while let Some(frame) = stream.next().await {
            let is_terminal = frame.is_terminal();
            match &frame {
                StreamFrame::Begin { .. } => {
                    begin_frame = Some(frame.clone());
                }
                StreamFrame::Data(data) => {
                    for chunk in data.chunks_exact(4) {
                        let s = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                        let abs = s.abs();
                        if abs > peak {
                            peak = abs;
                        }
                    }
                }
                _ => {}
            }
            all_chunks.push(frame);
            if is_terminal {
                break;
            }
        }

        // Compute gain
        #[cfg(feature = "av-core")]
        let target_linear = reflow_dsp::db::db_to_linear(target_db) as f32;
        #[cfg(not(feature = "av-core"))]
        let target_linear = 10.0_f32.powf(target_db as f32 / 20.0);

        let gain = if peak > 0.0 {
            target_linear / peak
        } else {
            1.0
        };

        // Pass 2: emit with gain applied
        for frame in all_chunks {
            let is_terminal = frame.is_terminal();
            let out_frame = match frame {
                StreamFrame::Data(data) => {
                    let mut samples: Vec<f32> = data
                        .chunks_exact(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect();
                    for s in &mut samples {
                        *s *= gain;
                    }
                    let bytes: Vec<u8> =
                        samples.iter().flat_map(|s| s.to_le_bytes()).collect();
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
