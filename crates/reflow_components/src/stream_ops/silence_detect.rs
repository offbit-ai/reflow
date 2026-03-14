//! Silence detection for audio streams.
//!
//! Passthrough actor that monitors audio level and emits events when
//! silence begins/ends. Useful for voice activity detection, automatic
//! recording start/stop, and skipping dead air.

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
    SilenceDetectActor,
    inports::<100>(stream),
    outports::<50>(stream, events, error),
    state(MemoryState)
)]
pub async fn silence_detect_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let threshold_db = config
        .get("thresholdDb")
        .and_then(|v| v.as_f64())
        .unwrap_or(-40.0);

    let min_duration_ms = config
        .get("minDurationMs")
        .and_then(|v| v.as_u64())
        .unwrap_or(500);

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

    let (events_tx, events_rx) = flume::bounded::<serde_json::Value>(64);

    #[cfg(feature = "av-core")]
    let threshold_linear = reflow_dsp::db::db_to_linear(threshold_db) as f32;
    #[cfg(not(feature = "av-core"))]
    let threshold_linear = 10.0_f32.powf(threshold_db as f32 / 20.0);

    let min_samples = (min_duration_ms as f64 * sample_rate / 1000.0) as u64;

    spawn_stream_task(async move {
        let mut stream = input_rx.into_stream();
        let mut is_silent = false;
        let mut silent_samples: u64 = 0;
        let mut total_samples: u64 = 0;
        let mut silence_regions: Vec<serde_json::Value> = Vec::new();
        let mut silence_start_sample: u64 = 0;

        while let Some(frame) = stream.next().await {
            let is_terminal = frame.is_terminal();

            if let StreamFrame::Data(ref data) = frame {
                let samples: Vec<f32> = data
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();

                for &s in &samples {
                    let level = s.abs();
                    total_samples += 1;

                    if level < threshold_linear {
                        if !is_silent {
                            silence_start_sample = total_samples;
                        }
                        silent_samples += 1;

                        if !is_silent && silent_samples >= min_samples {
                            is_silent = true;
                            let start_ms =
                                (silence_start_sample as f64 / sample_rate * 1000.0) as u64;
                            let _ = events_tx.try_send(json!({
                                "event": "silence.start",
                                "sampleOffset": silence_start_sample,
                                "timeMs": start_ms,
                            }));
                        }
                    } else {
                        if is_silent {
                            let end_ms = (total_samples as f64 / sample_rate * 1000.0) as u64;
                            let start_ms =
                                (silence_start_sample as f64 / sample_rate * 1000.0) as u64;
                            let _ = events_tx.try_send(json!({
                                "event": "silence.end",
                                "sampleOffset": total_samples,
                                "timeMs": end_ms,
                                "durationMs": end_ms - start_ms,
                            }));
                            silence_regions.push(json!({
                                "startMs": start_ms,
                                "endMs": end_ms,
                                "durationMs": end_ms - start_ms,
                            }));
                        }
                        is_silent = false;
                        silent_samples = 0;
                    }
                }
            }

            if tx.send_async(frame).await.is_err() || is_terminal {
                break;
            }
        }

        // Final summary
        let _ = events_tx.try_send(json!({
            "event": "silence.summary",
            "totalSamples": total_samples,
            "silenceRegions": silence_regions,
            "isSilentAtEnd": is_silent,
        }));
    });

    // Collect all events (non-blocking — events arrive during stream processing)
    // We return the handle immediately; events accumulate and are read after stream ends
    let events_rx_clone = events_rx.clone();
    spawn_stream_task(async move {
        // This task just keeps the events_rx alive until the stream ends
        let _ = events_rx_clone;
    });

    // Collect events that have arrived so far
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
