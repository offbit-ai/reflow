//! Frequency-selective compressor targeting sibilance (4–8 kHz).
//!
//! Uses a bandpass filter to isolate the sibilant band, an envelope
//! detector to measure its level, and applies gain reduction to the
//! full signal when sibilance exceeds the threshold.

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
    DeEsserActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn de_esser_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let frequency = config
        .get("frequency")
        .and_then(|v| v.as_f64())
        .unwrap_or(6000.0);

    let threshold_db = config
        .get("thresholdDb")
        .and_then(|v| v.as_f64())
        .unwrap_or(-20.0);

    let ratio = config
        .get("ratio")
        .and_then(|v| v.as_f64())
        .unwrap_or(4.0);

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
            use reflow_dsp::biquad::{Biquad, BiquadCoeffs, FilterType};
            use reflow_dsp::envelope::{DetectionMode, EnvelopeDetector};

            // Bandpass to isolate sibilance band
            let bp_coeffs = BiquadCoeffs::design(
                FilterType::BandPass,
                frequency,
                2.0, // moderate Q
                0.0,
                sample_rate,
            );
            let mut bp_filter = Biquad::new(bp_coeffs);

            // Envelope follower on the sibilant band
            let mut env = EnvelopeDetector::new(
                0.5,  // fast attack
                20.0, // moderate release
                sample_rate,
                DetectionMode::Peak,
            );

            let threshold_linear = reflow_dsp::db::db_to_linear(threshold_db) as f32;

            let mut stream = input_rx.into_stream();
            while let Some(frame) = stream.next().await {
                let is_terminal = frame.is_terminal();
                let out_frame = match frame {
                    StreamFrame::Data(data) => {
                        let mut samples: Vec<f32> = data
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();

                        for s in &mut samples {
                            // Detect sibilance level
                            let sibilant = bp_filter.process_sample(*s);
                            let level = env.process_sample(sibilant);

                            // Apply gain reduction when sibilance exceeds threshold
                            if level > threshold_linear {
                                let excess_db = 20.0 * (level / threshold_linear).log10();
                                let reduction_db = excess_db * (1.0 - 1.0 / ratio as f32);
                                let gain = 10.0_f32.powf(-reduction_db / 20.0);
                                *s *= gain;
                            }
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
        }

        #[cfg(not(feature = "av-core"))]
        {
            let _ = (frequency, threshold_db, ratio, sample_rate);
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
