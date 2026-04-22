//! FFT-based spectrum analysis for audio streams.
//!
//! Consumes PCM f32 audio, computes windowed FFT on each chunk, and
//! outputs magnitude spectrum as `audio/frequency-bins` stream frames
//! (f32 LE per bin). Also emits stats on completion.
//!
//! Designed for Zeal's spectrum visualization node.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use futures::StreamExt;
use reflow_actor::{
    message::EncodableValue,
    stream::{spawn_stream_task, StreamFrame},
    ActorContext,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    AudioSpectrumActor,
    inports::<100>(stream),
    outports::<50>(stream, stats, error),
    state(MemoryState)
)]
pub async fn audio_spectrum_actor(
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

    let sample_rate = config
        .get("sampleRate")
        .and_then(|v| v.as_f64())
        .unwrap_or(44100.0);

    let input_rx = match context.take_stream_receiver("stream") {
        Some(rx) => rx,
        None => return Ok(error_output("No StreamHandle on stream port")),
    };

    let bin_count = fft_size / 2 + 1;

    let (tx, handle) = context.create_stream(
        "stream",
        Some("audio/frequency-bins".to_string()),
        None,
        None,
    );

    let (stats_tx, stats_rx) = flume::bounded::<serde_json::Value>(1);

    spawn_stream_task(async move {
        // Emit Begin with spectrum metadata
        let _ = tx
            .send_async(StreamFrame::Begin {
                content_type: Some("audio/frequency-bins".to_string()),
                size_hint: None,
                metadata: Some(json!({
                    "fftSize": fft_size,
                    "binCount": bin_count,
                    "hopSize": hop_size,
                    "sampleRate": sample_rate,
                    "binResolution": sample_rate / fft_size as f64,
                })),
            })
            .await;

        #[cfg(feature = "av-core")]
        {
            let mut stft = reflow_dsp::fft::StftProcessor::new(
                fft_size,
                hop_size,
                reflow_dsp::window::WindowType::Hann,
            );

            let mut stream = input_rx.into_stream();
            let mut total_frames: u64 = 0;

            while let Some(frame) = stream.next().await {
                match frame {
                    StreamFrame::Data(data) => {
                        let samples: Vec<f32> = data
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();

                        // analyze() returns magnitude frames for each hop
                        let mag_frames = stft.analyze(&samples);
                        for mags in mag_frames {
                            total_frames += 1;
                            let bytes: Vec<u8> =
                                mags.iter().flat_map(|m: &f32| m.to_le_bytes()).collect();

                            if tx
                                .send_async(StreamFrame::Data(Arc::new(bytes)))
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                    StreamFrame::End => break,
                    StreamFrame::Error(e) => {
                        let _ = tx.send_async(StreamFrame::Error(e)).await;
                        let _ = stats_tx.send(json!({"error": true}));
                        return;
                    }
                    _ => {}
                }
            }

            let _ = tx.send_async(StreamFrame::End).await;
            let _ = stats_tx.send(json!({
                "totalSpectrumFrames": total_frames,
                "fftSize": fft_size,
                "binCount": bin_count,
                "hopSize": hop_size,
                "sampleRate": sample_rate,
            }));
        }

        #[cfg(not(feature = "av-core"))]
        {
            // Without FFT, just pass through and count
            let _ = (fft_size, hop_size, sample_rate, bin_count);
            let mut stream = input_rx.into_stream();
            let mut total_bytes: u64 = 0;
            while let Some(frame) = stream.next().await {
                let is_terminal = frame.is_terminal();
                if let StreamFrame::Data(ref d) = frame {
                    total_bytes += d.len() as u64;
                }
                if tx.send_async(frame).await.is_err() || is_terminal {
                    break;
                }
            }
            let _ = stats_tx.send(json!({"passthrough": true, "totalBytes": total_bytes}));
        }
    });

    let stats_value = stats_rx.recv_async().await.unwrap_or(json!({}));

    let mut results = HashMap::new();
    results.insert("stream".to_string(), Message::stream_handle(handle));
    results.insert(
        "stats".to_string(),
        Message::object(EncodableValue::from(stats_value)),
    );
    Ok(results)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
