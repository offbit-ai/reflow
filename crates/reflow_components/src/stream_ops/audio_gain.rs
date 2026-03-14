//! Applies gain (volume) to a PCM audio stream.
//!
//! Data frames are raw PCM f32 samples (little-endian). Supports both
//! linear gain and dB specification.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{
    stream::{spawn_stream_task, stream_transform},
    ActorContext,
};
use std::collections::HashMap;

#[actor(
    AudioGainActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn audio_gain_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    // Gain in dB (0.0 = unity, 6.0 ≈ double, -6.0 ≈ half)
    let gain_db = config
        .get("gainDb")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    // Or linear gain directly (overrides dB if set)
    let gain_linear = config
        .get("gainLinear")
        .and_then(|v| v.as_f64())
        .map(|g| g as f32)
        .unwrap_or_else(|| {
            #[cfg(feature = "av-core")]
            {
                reflow_dsp::db::db_to_linear(gain_db) as f32
            }
            #[cfg(not(feature = "av-core"))]
            {
                10.0_f32.powf(gain_db as f32 / 20.0)
            }
        });

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
        stream_transform(input_rx, tx, move |data: &[u8]| {
            // Data is LE f32 samples
            let mut samples: Vec<f32> = data
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();

            for s in &mut samples {
                *s *= gain_linear;
            }

            samples
                .iter()
                .flat_map(|s| s.to_le_bytes())
                .collect()
        })
        .await;
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
