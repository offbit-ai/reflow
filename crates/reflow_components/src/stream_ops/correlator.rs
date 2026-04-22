//! Cross-correlation between two audio streams.
//!
//! Computes normalized cross-correlation to estimate time delay and
//! similarity between two signals. Outputs a correlation stream and
//! delay estimate.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{
    message::EncodableValue,
    stream::{spawn_stream_task, stream_collect, StreamFrame},
    ActorContext,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    CorrelatorActor,
    inports::<100>(stream_a, stream_b),
    outports::<50>(stream, stats, error),
    state(MemoryState)
)]
pub async fn correlator_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let sample_rate = config
        .get("sampleRate")
        .and_then(|v| v.as_f64())
        .unwrap_or(44100.0);

    let max_lag_ms = config
        .get("maxLagMs")
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0);

    let rx_a = match context.take_stream_receiver("stream_a") {
        Some(rx) => rx,
        None => return Ok(error_output("No StreamHandle on stream_a port")),
    };
    let rx_b = match context.take_stream_receiver("stream_b") {
        Some(rx) => rx,
        None => return Ok(error_output("No StreamHandle on stream_b port")),
    };

    let (tx, handle) =
        context.create_stream("stream", Some("audio/correlation".to_string()), None, None);

    let (stats_tx, stats_rx) = flume::bounded::<serde_json::Value>(1);

    spawn_stream_task(async move {
        // Collect both streams
        let result_a = stream_collect(rx_a).await;
        let result_b = stream_collect(rx_b).await;

        let (samples_a, samples_b) = match (result_a, result_b) {
            (Ok((_, _, bytes_a)), Ok((_, _, bytes_b))) => {
                let a: Vec<f32> = bytes_a
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                let b: Vec<f32> = bytes_b
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                (a, b)
            }
            _ => {
                let _ = tx
                    .send_async(StreamFrame::Error("Failed to collect streams".to_string()))
                    .await;
                return;
            }
        };

        let max_lag = (max_lag_ms * sample_rate / 1000.0) as usize;
        let len = samples_a.len().min(samples_b.len());

        if len == 0 {
            let _ = tx
                .send_async(StreamFrame::Error("Empty input streams".to_string()))
                .await;
            return;
        }

        // Compute RMS for normalization
        let rms_a: f32 = (samples_a[..len].iter().map(|s| s * s).sum::<f32>() / len as f32).sqrt();
        let rms_b: f32 = (samples_b[..len].iter().map(|s| s * s).sum::<f32>() / len as f32).sqrt();
        let norm = rms_a * rms_b * len as f32;

        // Cross-correlation for -max_lag..+max_lag
        let mut correlations: Vec<f32> = Vec::with_capacity(max_lag * 2 + 1);
        let mut best_lag: i32 = 0;
        let mut best_corr: f32 = f32::NEG_INFINITY;

        for lag in -(max_lag as i32)..=(max_lag as i32) {
            let mut sum: f32 = 0.0;
            let mut count = 0;
            for i in 0..len {
                let j = i as i32 + lag;
                if j >= 0 && (j as usize) < len {
                    sum += samples_a[i] * samples_b[j as usize];
                    count += 1;
                }
            }
            let corr = if norm > 0.0 && count > 0 {
                sum / norm
            } else {
                0.0
            };
            correlations.push(corr);
            if corr > best_corr {
                best_corr = corr;
                best_lag = lag;
            }
        }

        // Emit correlation as stream
        let _ = tx
            .send_async(StreamFrame::Begin {
                content_type: Some("audio/correlation".to_string()),
                size_hint: None,
                metadata: Some(json!({
                    "maxLag": max_lag,
                    "sampleRate": sample_rate,
                })),
            })
            .await;

        let bytes: Vec<u8> = correlations.iter().flat_map(|c| c.to_le_bytes()).collect();
        let _ = tx.send_async(StreamFrame::Data(Arc::new(bytes))).await;
        let _ = tx.send_async(StreamFrame::End).await;

        let delay_ms = best_lag as f64 / sample_rate * 1000.0;
        let _ = stats_tx.send(json!({
            "bestLagSamples": best_lag,
            "bestLagMs": delay_ms,
            "peakCorrelation": best_corr,
            "signalLength": len,
        }));
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
