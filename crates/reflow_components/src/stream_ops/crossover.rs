//! Multi-band frequency crossover — splits audio into low, mid, high bands.
//!
//! Uses Linkwitz-Riley crossover (cascaded biquad pairs) for flat
//! magnitude response when bands are summed.

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
    CrossoverActor,
    inports::<100>(stream),
    outports::<50>(low, mid, high, error),
    state(MemoryState)
)]
pub async fn crossover_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let low_freq = config
        .get("lowFrequency")
        .and_then(|v| v.as_f64())
        .unwrap_or(200.0);

    let high_freq = config
        .get("highFrequency")
        .and_then(|v| v.as_f64())
        .unwrap_or(4000.0);

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

    let (tx_low, handle_low) =
        context.create_stream("low", input_handle.content_type.clone(), None, None);
    let (tx_mid, handle_mid) =
        context.create_stream("mid", input_handle.content_type.clone(), None, None);
    let (tx_high, handle_high) =
        context.create_stream("high", input_handle.content_type.clone(), None, None);

    spawn_stream_task(async move {
        #[cfg(feature = "av-core")]
        {
            use reflow_dsp::biquad::{Biquad, BiquadCoeffs, FilterType};

            // Linkwitz-Riley: two cascaded biquads per crossover point
            let lp1_coeffs =
                BiquadCoeffs::design(FilterType::LowPass, low_freq, 0.707, 0.0, sample_rate);
            let lp2_coeffs = lp1_coeffs.clone();
            let hp1_coeffs =
                BiquadCoeffs::design(FilterType::HighPass, low_freq, 0.707, 0.0, sample_rate);
            let hp2_coeffs = hp1_coeffs.clone();
            let lp3_coeffs =
                BiquadCoeffs::design(FilterType::LowPass, high_freq, 0.707, 0.0, sample_rate);
            let lp4_coeffs = lp3_coeffs.clone();
            let hp3_coeffs =
                BiquadCoeffs::design(FilterType::HighPass, high_freq, 0.707, 0.0, sample_rate);
            let hp4_coeffs = hp3_coeffs.clone();

            // Low: LPF @ low_freq (LR4)
            let mut lp1 = Biquad::new(lp1_coeffs);
            let mut lp2 = Biquad::new(lp2_coeffs);
            // Mid-high: HPF @ low_freq, then split
            let mut hp1 = Biquad::new(hp1_coeffs);
            let mut hp2 = Biquad::new(hp2_coeffs);
            // Mid: LPF @ high_freq (from mid-high)
            let mut lp3 = Biquad::new(lp3_coeffs);
            let mut lp4 = Biquad::new(lp4_coeffs);
            // High: HPF @ high_freq (from mid-high)
            let mut hp3 = Biquad::new(hp3_coeffs);
            let mut hp4 = Biquad::new(hp4_coeffs);

            let mut stream = input_rx.into_stream();
            while let Some(frame) = stream.next().await {
                let is_terminal = frame.is_terminal();
                match frame {
                    StreamFrame::Data(data) => {
                        let samples: Vec<f32> = data
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();

                        let mut low_samples = samples.clone();
                        lp1.process(&mut low_samples);
                        lp2.process(&mut low_samples);

                        let mut mid_high = samples;
                        hp1.process(&mut mid_high);
                        hp2.process(&mut mid_high);

                        let mut mid_samples = mid_high.clone();
                        lp3.process(&mut mid_samples);
                        lp4.process(&mut mid_samples);

                        let mut high_samples = mid_high;
                        hp3.process(&mut high_samples);
                        hp4.process(&mut high_samples);

                        let to_bytes = |s: &[f32]| -> Vec<u8> {
                            s.iter().flat_map(|v| v.to_le_bytes()).collect()
                        };

                        let _ = tx_low
                            .send_async(StreamFrame::Data(Arc::new(to_bytes(&low_samples))))
                            .await;
                        let _ = tx_mid
                            .send_async(StreamFrame::Data(Arc::new(to_bytes(&mid_samples))))
                            .await;
                        let _ = tx_high
                            .send_async(StreamFrame::Data(Arc::new(to_bytes(&high_samples))))
                            .await;
                    }
                    StreamFrame::End => {
                        let _ = tx_low.send_async(StreamFrame::End).await;
                        let _ = tx_mid.send_async(StreamFrame::End).await;
                        let _ = tx_high.send_async(StreamFrame::End).await;
                        break;
                    }
                    StreamFrame::Error(e) => {
                        let _ = tx_low.send_async(StreamFrame::Error(e.clone())).await;
                        let _ = tx_mid.send_async(StreamFrame::Error(e.clone())).await;
                        let _ = tx_high.send_async(StreamFrame::Error(e)).await;
                        break;
                    }
                    other => {
                        let _ = tx_low.send_async(other.clone()).await;
                        let _ = tx_mid.send_async(other.clone()).await;
                        let _ = tx_high.send_async(other).await;
                        if is_terminal {
                            break;
                        }
                    }
                }
            }
        }

        #[cfg(not(feature = "av-core"))]
        {
            let _ = (low_freq, high_freq, sample_rate);
            // Passthrough to all three outputs
            let mut stream = input_rx.into_stream();
            while let Some(frame) = stream.next().await {
                let is_terminal = frame.is_terminal();
                let _ = tx_low.send_async(frame.clone()).await;
                let _ = tx_mid.send_async(frame.clone()).await;
                if tx_high.send_async(frame).await.is_err() || is_terminal {
                    break;
                }
            }
        }
    });

    let mut results = HashMap::new();
    results.insert("low".to_string(), Message::stream_handle(handle_low));
    results.insert("mid".to_string(), Message::stream_handle(handle_mid));
    results.insert("high".to_string(), Message::stream_handle(handle_high));
    Ok(results)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
