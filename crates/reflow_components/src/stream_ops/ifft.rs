//! Inverse FFT — converts frequency-domain bins back to PCM audio.
//!
//! Consumes `audio/frequency-bins` stream (f32 magnitude per bin) and
//! reconstructs time-domain audio via overlap-add ISTFT.

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
    IFFTActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn ifft_actor(
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

    let input_rx = match context.take_stream_receiver("stream") {
        Some(rx) => rx,
        None => return Ok(error_output("No StreamHandle on stream port")),
    };

    let (tx, handle) = context.create_stream(
        "stream",
        Some("audio/raw-pcm-f32".to_string()),
        None,
        None,
    );

    spawn_stream_task(async move {
        let _ = tx
            .send_async(StreamFrame::Begin {
                content_type: Some("audio/raw-pcm-f32".to_string()),
                size_hint: None,
                metadata: Some(serde_json::json!({
                    "fftSize": fft_size,
                    "hopSize": hop_size,
                })),
            })
            .await;

        #[cfg(feature = "av-core")]
        {
            use reflow_dsp::realfft::RealFftPlanner;
            use reflow_dsp::rustfft::num_complex::Complex;

            let bin_count = fft_size / 2 + 1;
            let window = reflow_dsp::window::generate(
                reflow_dsp::window::WindowType::Hann,
                fft_size,
            );

            let mut planner = RealFftPlanner::<f32>::new();
            let ifft = planner.plan_fft_inverse(fft_size);
            let mut ifft_input = vec![Complex::default(); bin_count];
            let mut ifft_output = vec![0.0f32; fft_size];

            // Overlap-add buffer
            let mut ola_buf = vec![0.0f32; fft_size * 2];
            let mut ola_pos: usize = 0;
            let mut output_pos: usize = 0;
            let norm = 1.0 / fft_size as f32;

            let mut stream = input_rx.into_stream();
            while let Some(frame) = stream.next().await {
                match frame {
                    StreamFrame::Data(data) => {
                        // Input is magnitude-only — assume zero phase
                        let magnitudes: Vec<f32> = data
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();

                        // Fill complex bins (magnitude + zero phase)
                        for (i, bin) in ifft_input.iter_mut().enumerate() {
                            let mag = magnitudes.get(i).copied().unwrap_or(0.0);
                            *bin = Complex::new(mag, 0.0);
                        }

                        // Inverse FFT
                        let _ = ifft.process(&mut ifft_input, &mut ifft_output);

                        // Apply synthesis window and overlap-add
                        for (i, &s) in ifft_output.iter().enumerate() {
                            let idx = (ola_pos + i) % ola_buf.len();
                            ola_buf[idx] += s * norm * window[i];
                        }

                        // Read out hop_size samples
                        let mut out = Vec::with_capacity(hop_size);
                        for _ in 0..hop_size {
                            let idx = output_pos % ola_buf.len();
                            out.push(ola_buf[idx]);
                            ola_buf[idx] = 0.0; // clear for next overlap
                            output_pos += 1;
                        }
                        ola_pos += hop_size;

                        let bytes: Vec<u8> =
                            out.iter().flat_map(|s| s.to_le_bytes()).collect();
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
            let _ = (fft_size, hop_size);
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
