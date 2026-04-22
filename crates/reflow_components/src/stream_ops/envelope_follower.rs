//! Extracts amplitude envelope from audio stream.
//!
//! Outputs `audio/control-signal` — one f32 per input sample representing
//! the instantaneous envelope level. Used for sidechain compression,
//! ducking, visualization, and modulation routing.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use futures::StreamExt;
use reflow_actor::{
    stream::{spawn_stream_task, StreamFrame},
    ActorContext,
};
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    EnvelopeFollowerActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn envelope_follower_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let attack_ms = config
        .get("attackMs")
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0);

    let release_ms = config
        .get("releaseMs")
        .and_then(|v| v.as_f64())
        .unwrap_or(50.0);

    let mode = config
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("peak")
        .to_string();

    let sample_rate = config
        .get("sampleRate")
        .and_then(|v| v.as_f64())
        .unwrap_or(44100.0);

    let input_rx = match context.take_stream_receiver("stream") {
        Some(rx) => rx,
        None => return Ok(error_output("No StreamHandle on stream port")),
    };

    let (tx, handle) = context.create_stream(
        "stream",
        Some("audio/control-signal".to_string()),
        None,
        None,
    );

    spawn_stream_task(async move {
        // Emit Begin with control-signal metadata
        let _ = tx
            .send_async(StreamFrame::Begin {
                content_type: Some("audio/control-signal".to_string()),
                size_hint: None,
                metadata: Some(serde_json::json!({
                    "attackMs": attack_ms,
                    "releaseMs": release_ms,
                    "mode": mode,
                    "sampleRate": sample_rate,
                })),
            })
            .await;

        #[cfg(feature = "av-core")]
        {
            use reflow_dsp::envelope::{DetectionMode, EnvelopeDetector};

            let det_mode = if mode == "rms" {
                DetectionMode::Rms
            } else {
                DetectionMode::Peak
            };
            let mut detector = EnvelopeDetector::new(attack_ms, release_ms, sample_rate, det_mode);

            let mut stream = input_rx.into_stream();
            while let Some(frame) = stream.next().await {
                match frame {
                    StreamFrame::Data(data) => {
                        let samples: Vec<f32> = data
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();
                        let envelope = detector.process_to_vec(&samples);
                        let bytes: Vec<u8> =
                            envelope.iter().flat_map(|s| s.to_le_bytes()).collect();
                        if tx
                            .send_async(StreamFrame::Data(Arc::new(bytes)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    StreamFrame::End => {
                        let _ = tx.send_async(StreamFrame::End).await;
                        break;
                    }
                    StreamFrame::Error(e) => {
                        let _ = tx.send_async(StreamFrame::Error(e)).await;
                        break;
                    }
                    _ => {}
                }
            }
        }

        #[cfg(not(feature = "av-core"))]
        {
            // Simple peak follower fallback
            let attack_coeff = (-1.0 / (attack_ms * sample_rate as f64 / 1000.0)).exp() as f32;
            let release_coeff = (-1.0 / (release_ms * sample_rate as f64 / 1000.0)).exp() as f32;
            let mut level: f32 = 0.0;

            let mut stream = input_rx.into_stream();
            while let Some(frame) = stream.next().await {
                match frame {
                    StreamFrame::Data(data) => {
                        let samples: Vec<f32> = data
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();
                        let mut envelope = Vec::with_capacity(samples.len());
                        for s in &samples {
                            let abs = s.abs();
                            if abs > level {
                                level = attack_coeff * level + (1.0 - attack_coeff) * abs;
                            } else {
                                level = release_coeff * level + (1.0 - release_coeff) * abs;
                            }
                            envelope.push(level);
                        }
                        let bytes: Vec<u8> =
                            envelope.iter().flat_map(|s| s.to_le_bytes()).collect();
                        if tx
                            .send_async(StreamFrame::Data(Arc::new(bytes)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    StreamFrame::End => {
                        let _ = tx.send_async(StreamFrame::End).await;
                        break;
                    }
                    StreamFrame::Error(e) => {
                        let _ = tx.send_async(StreamFrame::Error(e)).await;
                        break;
                    }
                    _ => {}
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
