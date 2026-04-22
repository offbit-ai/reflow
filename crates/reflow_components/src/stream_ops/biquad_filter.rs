//! Configurable biquad filter for audio streams.
//!
//! Supports LowPass, HighPass, BandPass, Notch, PeakingEQ, and shelf
//! types. Data frames are raw PCM f32 samples (little-endian).

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
    BiquadFilterActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn biquad_filter_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let filter_type_str = config
        .get("filterType")
        .and_then(|v| v.as_str())
        .unwrap_or("lowpass")
        .to_string();

    let frequency = config
        .get("frequency")
        .and_then(|v| v.as_f64())
        .unwrap_or(1000.0);

    let q = config.get("q").and_then(|v| v.as_f64()).unwrap_or(0.707);

    let gain_db = config.get("gainDb").and_then(|v| v.as_f64()).unwrap_or(0.0);

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
            let filter_type = match filter_type_str.as_str() {
                "highpass" | "hpf" => reflow_dsp::biquad::FilterType::HighPass,
                "bandpass" | "bpf" => reflow_dsp::biquad::FilterType::BandPass,
                "notch" => reflow_dsp::biquad::FilterType::Notch,
                "peaking" | "eq" => reflow_dsp::biquad::FilterType::PeakingEQ,
                "lowshelf" => reflow_dsp::biquad::FilterType::LowShelf,
                "highshelf" => reflow_dsp::biquad::FilterType::HighShelf,
                _ => reflow_dsp::biquad::FilterType::LowPass,
            };

            let coeffs = reflow_dsp::biquad::BiquadCoeffs::design(
                filter_type,
                frequency,
                q,
                gain_db,
                sample_rate,
            );
            let mut filter = reflow_dsp::biquad::Biquad::new(coeffs);

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
            // Passthrough when av-core is disabled
            let _ = (filter_type_str, frequency, q, gain_db, sample_rate);
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
