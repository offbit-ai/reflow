//! Multi-band parametric equalizer.
//!
//! Chains N biquad filters in series, each with independent frequency,
//! gain, Q, and type. Configured via a JSON array of band descriptors.

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
    EqualizerActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn equalizer_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let sample_rate = config
        .get("sampleRate")
        .and_then(|v| v.as_f64())
        .unwrap_or(44100.0);

    // bands: [{ "type": "peaking", "frequency": 1000, "gain": 3.0, "q": 1.0 }, ...]
    let bands_json = config
        .get("bands")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));

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

            let mut filters: Vec<Biquad> = Vec::new();

            if let Some(arr) = bands_json.as_array() {
                for band in arr {
                    let ft = match band
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("peaking")
                    {
                        "lowpass" | "lpf" => FilterType::LowPass,
                        "highpass" | "hpf" => FilterType::HighPass,
                        "bandpass" | "bpf" => FilterType::BandPass,
                        "notch" => FilterType::Notch,
                        "lowshelf" => FilterType::LowShelf,
                        "highshelf" => FilterType::HighShelf,
                        _ => FilterType::PeakingEQ,
                    };
                    let freq = band
                        .get("frequency")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(1000.0);
                    let gain = band.get("gain").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let q = band.get("q").and_then(|v| v.as_f64()).unwrap_or(1.0);

                    let coeffs = BiquadCoeffs::design(ft, freq, q, gain, sample_rate);
                    filters.push(Biquad::new(coeffs));
                }
            }

            let mut stream = input_rx.into_stream();
            while let Some(frame) = stream.next().await {
                let is_terminal = frame.is_terminal();
                let out_frame = match frame {
                    StreamFrame::Data(data) => {
                        let mut samples: Vec<f32> = data
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();
                        for filter in &mut filters {
                            filter.process(&mut samples);
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

        #[cfg(not(feature = "av-core"))]
        {
            let _ = (sample_rate, bands_json);
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
