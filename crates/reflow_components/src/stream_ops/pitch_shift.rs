//! Phase vocoder pitch shifting.
//!
//! Shifts pitch by a configurable semitone amount using STFT-based
//! phase vocoder with frequency-domain bin shifting.

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

#[actor(
    PitchShiftActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn pitch_shift_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    // Semitones to shift (positive = up, negative = down)
    let semitones = config
        .get("semitones")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let fft_size = config
        .get("fftSize")
        .and_then(|v| v.as_u64())
        .unwrap_or(4096) as usize;

    let hop_size = config
        .get("hopSize")
        .and_then(|v| v.as_u64())
        .unwrap_or(256) as usize;

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
            let shift_ratio = 2.0_f64.powf(semitones / 12.0);

            let mut stft = reflow_dsp::fft::StftProcessor::new(
                fft_size,
                hop_size,
                reflow_dsp::window::WindowType::Hann,
            );

            let bin_count = fft_size / 2 + 1;

            let mut stream = input_rx.into_stream();
            while let Some(frame) = stream.next().await {
                let is_terminal = frame.is_terminal();
                let out_frame = match frame {
                    StreamFrame::Data(data) => {
                        let input: Vec<f32> = data
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();

                        let mut output = Vec::new();
                        let bc = bin_count;
                        let sr = shift_ratio as f32;
                        stft.process(&input, &mut output, move |frame| {
                            let mags = frame.magnitudes();
                            let phases = frame.phases();

                            // Shift bins by ratio
                            let mut new_mags = vec![0.0f32; bc];
                            let mut new_phases = vec![0.0f32; bc];

                            for (i, (&m, &p)) in mags.iter().zip(phases.iter()).enumerate() {
                                let new_bin = (i as f32 * sr) as usize;
                                if new_bin < bc {
                                    new_mags[new_bin] += m;
                                    new_phases[new_bin] = p;
                                }
                            }

                            reflow_dsp::fft::FftFrame::from_polar(&new_mags, &new_phases)
                        });

                        let bytes: Vec<u8> = output.iter().flat_map(|s| s.to_le_bytes()).collect();
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
            let _ = (semitones, fft_size, hop_size);
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
