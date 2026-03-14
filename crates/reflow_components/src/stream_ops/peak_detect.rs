//! Onset/transient detection for audio streams.
//!
//! Passthrough actor that detects sudden increases in energy (transients)
//! and emits events. Used for beat tracking, automatic segmentation,
//! and triggering downstream actions on hits.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{
    message::EncodableValue,
    stream::{spawn_stream_task, StreamFrame},
    ActorContext,
};
use futures::StreamExt;
use serde_json::json;
use std::collections::HashMap;

#[actor(
    PeakDetectActor,
    inports::<100>(stream),
    outports::<50>(stream, events, error),
    state(MemoryState)
)]
pub async fn peak_detect_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    // Sensitivity: how many times the running average a transient must exceed
    let sensitivity = config
        .get("sensitivity")
        .and_then(|v| v.as_f64())
        .unwrap_or(3.0) as f32;

    // Window size for running average (in samples)
    let window_samples = config
        .get("windowMs")
        .and_then(|v| v.as_f64())
        .unwrap_or(50.0);

    let sample_rate = config
        .get("sampleRate")
        .and_then(|v| v.as_f64())
        .unwrap_or(44100.0);

    let min_interval_ms = config
        .get("minIntervalMs")
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0);

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

    let (events_tx, events_rx) = flume::bounded::<serde_json::Value>(256);

    let avg_window = (window_samples * sample_rate / 1000.0) as usize;
    let min_interval_samples = (min_interval_ms * sample_rate / 1000.0) as u64;

    spawn_stream_task(async move {
        let mut stream = input_rx.into_stream();
        let mut running_energy: f32 = 0.0;
        let energy_coeff = if avg_window > 0 { 1.0 / avg_window as f32 } else { 1.0 };
        let mut total_samples: u64 = 0;
        let mut last_peak_sample: u64 = 0;
        let mut peaks: Vec<serde_json::Value> = Vec::new();

        while let Some(frame) = stream.next().await {
            let is_terminal = frame.is_terminal();

            if let StreamFrame::Data(ref data) = frame {
                let samples: Vec<f32> = data
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();

                for &s in &samples {
                    let energy = s * s;
                    total_samples += 1;

                    // Exponential moving average of energy
                    running_energy = running_energy * (1.0 - energy_coeff) + energy * energy_coeff;

                    // Detect transient
                    if energy > running_energy * sensitivity * sensitivity
                        && (total_samples - last_peak_sample) >= min_interval_samples
                    {
                        last_peak_sample = total_samples;
                        let time_ms = (total_samples as f64 / sample_rate * 1000.0) as u64;
                        let peak = json!({
                            "event": "peak",
                            "sampleOffset": total_samples,
                            "timeMs": time_ms,
                            "energy": energy,
                        });
                        let _ = events_tx.try_send(peak.clone());
                        peaks.push(peak);
                    }
                }
            }

            if tx.send_async(frame).await.is_err() || is_terminal {
                break;
            }
        }

        let _ = events_tx.try_send(json!({
            "event": "summary",
            "totalPeaks": peaks.len(),
            "peaks": peaks,
        }));
    });

    let mut event_list = Vec::new();
    while let Ok(evt) = events_rx.try_recv() {
        event_list.push(evt);
    }

    let mut results = HashMap::new();
    results.insert("stream".to_string(), Message::stream_handle(handle));
    results.insert(
        "events".to_string(),
        Message::object(EncodableValue::from(json!(event_list))),
    );
    Ok(results)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
