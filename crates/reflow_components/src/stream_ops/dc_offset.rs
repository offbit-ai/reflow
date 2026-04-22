//! DC offset removal via single-pole high-pass filter.
//!
//! Removes any DC bias from the audio signal. Uses a very low cutoff
//! (default 5 Hz) to only affect the DC component.

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
    DCOffsetActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn dc_offset_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let cutoff = config
        .get("cutoffHz")
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0);

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

            let coeffs =
                BiquadCoeffs::design(FilterType::HighPass, cutoff, 0.707, 0.0, sample_rate);
            let mut filter = Biquad::new(coeffs);

            let mut stream = input_rx.into_stream();
            while let Some(frame) = stream.next().await {
                let is_terminal = frame.is_terminal();
                let out_frame = match frame {
                    StreamFrame::Data(data) => {
                        let mut samples: Vec<f32> = data
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();
                        filter.process(&mut samples);
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
            // Simple single-pole HPF: y[n] = x[n] - x[n-1] + R * y[n-1]
            let r = 1.0 - (2.0 * std::f64::consts::PI * cutoff / sample_rate) as f32;
            let mut prev_in: f32 = 0.0;
            let mut prev_out: f32 = 0.0;

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
                            let out = *s - prev_in + r * prev_out;
                            prev_in = *s;
                            prev_out = out;
                            *s = out;
                        }
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
