//! Noise gate for audio streams.
//!
//! Attenuates audio below a threshold. Companion to CompressorActor —
//! compressor tames peaks, gate silences noise floor.

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
    NoiseGateActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn noise_gate_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let threshold_db = config
        .get("thresholdDb")
        .and_then(|v| v.as_f64())
        .unwrap_or(-40.0);

    let ratio = config.get("ratio").and_then(|v| v.as_f64()).unwrap_or(10.0);

    let attack_ms = config
        .get("attackMs")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);

    let release_ms = config
        .get("releaseMs")
        .and_then(|v| v.as_f64())
        .unwrap_or(50.0);

    let sample_rate = config
        .get("sampleRate")
        .and_then(|v| v.as_f64())
        .unwrap_or(44100.0);

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
        #[cfg(feature = "av-core")]
        {
            let mut gate = reflow_dsp::envelope::DynamicsProcessor::gate(
                threshold_db,
                ratio,
                attack_ms,
                release_ms,
                sample_rate,
            );

            let mut stream = input_rx.into_stream();
            while let Some(frame) = stream.next().await {
                let is_terminal = frame.is_terminal();
                let out_frame = match frame {
                    StreamFrame::Data(data) => {
                        let mut samples: Vec<f32> = data
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();
                        gate.process(&mut samples);
                        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
                        StreamFrame::Data(Arc::new(bytes))
                    }
                    other => other,
                };
                if tx.send_async(out_frame).await.is_err() || is_terminal {
                    break;
                }
            }
        }

        #[cfg(not(feature = "av-core"))]
        {
            let _ = (threshold_db, ratio, attack_ms, release_ms, sample_rate);
            let mut stream = input_rx.into_stream();
            while let Some(frame) = stream.next().await {
                let is_terminal = frame.is_terminal();
                if tx.send_async(frame).await.is_err() || is_terminal {
                    break;
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
