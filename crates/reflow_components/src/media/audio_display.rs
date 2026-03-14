//! Audio stream display actor — consumes an audio stream and emits metadata
//! for Zeal's audio display node.
//!
//! Follows the same pattern as `ImageStreamDisplayActor`: receives a
//! `StreamHandle`, drains frames (observer tap handles Zeal forwarding),
//! and outputs audio metadata for downstream nodes.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, stream::StreamFrame, ActorContext};
use serde_json::json;
use std::collections::HashMap;

const DEFAULT_AUDIO_CONTENT_TYPES: &[&str] = &[
    "audio/mpeg",
    "audio/wav",
    "audio/ogg",
    "audio/webm",
    "audio/aac",
    "audio/flac",
    "audio/raw-pcm",
];

/// Audio Stream Display Actor — compatible with `tpl_audio_stream_display`
///
/// Consumes an audio stream handle and drains it, allowing the observer
/// tap to forward binary frames to Zeal for live playback / waveform
/// visualization. Outputs audio metadata for downstream use.
#[actor(
    AudioStreamDisplayActor,
    inports::<100>(stream),
    outports::<50>(metadata, error),
    state(MemoryState)
)]
pub async fn audio_stream_display_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let property_values = config.get("propertyValues").and_then(|v| v.as_object());

    let autoplay = property_values
        .and_then(|pv| pv.get("autoplay"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let loop_playback = property_values
        .and_then(|pv| pv.get("loop"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let show_waveform = property_values
        .and_then(|pv| pv.get("showWaveform"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Take the stream receiver
    let stream_rx = context
        .take_stream_receiver("stream")
        .ok_or_else(|| anyhow::anyhow!("No stream handle on 'stream' port"))?;

    // Read Begin frame for metadata
    let first = stream_rx
        .recv_async()
        .await
        .map_err(|_| anyhow::anyhow!("Stream closed before Begin frame"))?;

    let (content_type, size_hint, stream_meta) = match first {
        StreamFrame::Begin {
            content_type,
            size_hint,
            metadata,
        } => (content_type, size_hint, metadata),
        StreamFrame::Error(e) => return Ok(error_output(format!("Stream error: {}", e))),
        _ => return Ok(error_output("Expected Begin frame, got data".into())),
    };

    let ct = content_type.as_deref().unwrap_or("application/octet-stream");

    // Validate audio content type
    let accepted: Vec<String> = property_values
        .and_then(|pv| pv.get("acceptedFormats"))
        .and_then(|v| v.as_str())
        .map(|s| s.split(',').map(|f| f.trim().to_string()).collect())
        .unwrap_or_else(|| {
            DEFAULT_AUDIO_CONTENT_TYPES
                .iter()
                .map(|s| s.to_string())
                .collect()
        });

    if !accepted.iter().any(|f| ct.starts_with(f.as_str())) {
        return Ok(error_output(format!(
            "Unsupported audio stream format: {}",
            ct
        )));
    }

    // Extract audio-specific metadata from stream
    let sample_rate = stream_meta
        .as_ref()
        .and_then(|m| m.get("sampleRate"))
        .and_then(|v| v.as_u64());
    let channels = stream_meta
        .as_ref()
        .and_then(|m| m.get("channels"))
        .and_then(|v| v.as_u64());
    let bit_depth = stream_meta
        .as_ref()
        .and_then(|m| m.get("bitDepth"))
        .and_then(|v| v.as_u64());
    let duration_ms = stream_meta
        .as_ref()
        .and_then(|m| m.get("durationMs"))
        .and_then(|v| v.as_u64());

    // Drain remaining frames to maintain backpressure
    let mut total_bytes: u64 = 0;
    let mut chunk_count: u64 = 0;
    loop {
        match stream_rx.recv_async().await {
            Ok(StreamFrame::Data(data)) => {
                total_bytes += data.len() as u64;
                chunk_count += 1;
            }
            Ok(StreamFrame::End) => break,
            Ok(StreamFrame::Error(e)) => {
                return Ok(error_output(format!("Stream error during transfer: {}", e)));
            }
            Ok(StreamFrame::Begin { .. }) => {}
            Err(_) => break,
        }
    }

    let metadata = json!({
        "contentType": ct,
        "sizeHint": size_hint,
        "totalBytes": total_bytes,
        "chunks": chunk_count,
        "sampleRate": sample_rate,
        "channels": channels,
        "bitDepth": bit_depth,
        "durationMs": duration_ms,
        "playback": {
            "autoplay": autoplay,
            "loop": loop_playback,
            "showWaveform": show_waveform,
        },
        "streamMetadata": stream_meta,
    });

    let mut output = HashMap::new();
    output.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(metadata)),
    );
    Ok(output)
}

fn error_output(msg: String) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.into()));
    out
}
