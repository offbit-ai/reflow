//! Spectral subtraction noise reduction.
//!
//! Learns a noise profile from the first N ms of audio (assumed to be
//! silence/noise-only), then subtracts that spectral profile from all
//! subsequent frames.

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
    NoiseReductionActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn noise_reduction_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let fft_size = config
        .get("fftSize")
        .and_then(|v| v.as_u64())
        .unwrap_or(2048) as usize;

    let hop_size = config
        .get("hopSize")
        .and_then(|v| v.as_u64())
        .unwrap_or(512) as usize;

    // How many ms of initial audio to use for noise profile
    let profile_ms = config
        .get("profileMs")
        .and_then(|v| v.as_f64())
        .unwrap_or(500.0);

    // Reduction strength (1.0 = full subtraction, 0.5 = half)
    let strength = config
        .get("strength")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;

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
            let mut stft = reflow_dsp::fft::StftProcessor::new(
                fft_size,
                hop_size,
                reflow_dsp::window::WindowType::Hann,
            );

            let profile_samples = (profile_ms * sample_rate / 1000.0) as usize;
            let bin_count = fft_size / 2 + 1;
            let mut noise_profile: Vec<f32> = vec![0.0; bin_count];
            let mut profile_frames: usize = 0;
            let mut samples_seen: usize = 0;
            let mut profiling = true;

            let mut stream = input_rx.into_stream();
            while let Some(frame) = stream.next().await {
                let is_terminal = frame.is_terminal();
                let out_frame = match frame {
                    StreamFrame::Data(data) => {
                        let input: Vec<f32> = data
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();

                        samples_seen += input.len();

                        if profiling {
                            // Accumulate noise profile
                            let mag_frames = stft.analyze(&input);
                            for mags in &mag_frames {
                                for (i, &m) in mags.iter().enumerate() {
                                    if i < noise_profile.len() {
                                        noise_profile[i] += m;
                                    }
                                }
                                profile_frames += 1;
                            }

                            if samples_seen >= profile_samples {
                                // Average the noise profile
                                if profile_frames > 0 {
                                    for v in &mut noise_profile {
                                        *v /= profile_frames as f32;
                                    }
                                }
                                profiling = false;
                                // Reset STFT for clean processing
                                stft.reset();
                            }

                            // During profiling, pass through unmodified
                            StreamFrame::Data(data)
                        } else {
                            // Spectral subtraction via STFT process
                            let noise_ref = noise_profile.clone();
                            let s = strength;
                            let mut output = Vec::new();
                            stft.process(&input, &mut output, |frame| {
                                let mags = frame.magnitudes();
                                let phases = frame.phases();
                                let new_mags: Vec<f32> = mags
                                    .iter()
                                    .enumerate()
                                    .map(|(i, &m)| {
                                        let noise = noise_ref.get(i).copied().unwrap_or(0.0);
                                        (m - noise * s).max(0.0)
                                    })
                                    .collect();
                                reflow_dsp::fft::FftFrame::from_polar(&new_mags, &phases)
                            });

                            let bytes: Vec<u8> =
                                output.iter().flat_map(|s| s.to_le_bytes()).collect();
                            StreamFrame::Data(Arc::new(bytes))
                        }
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
            let _ = (fft_size, hop_size, profile_ms, strength, sample_rate);
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
