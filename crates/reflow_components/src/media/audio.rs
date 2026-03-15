//! Audio input actor — fetches, validates, and extracts metadata from audio.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

const DEFAULT_ACCEPTED_FORMATS: &[&str] = &["audio/mpeg", "audio/wav", "audio/ogg", "audio/webm"];
const DEFAULT_MAX_FILE_SIZE_MB: u64 = 50;
const DEFAULT_TIMEOUT_MS: u64 = 60_000;

/// Audio Input Actor — compatible with `tpl_audio_input`
///
/// All source modes (url, upload, record) resolve to a URL —
/// Zeal handles uploads/recordings to S3 and provides the resulting URL.
/// The actor fetches audio metadata via HEAD, validates format/size,
/// and outputs audio data + metadata.
#[actor(
    AudioInputActor,
    inports::<100>(source),
    outports::<50>(audioData, metadata, error),
    state(MemoryState)
)]
pub async fn audio_input_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let inputs = context.get_payload();
    let config = context.get_config_hashmap();
    let accepted_formats: Vec<String> = config
        .get("acceptedFormats")
        .and_then(|v| v.as_str())
        .map(|s| s.split(',').map(|f| f.trim().to_string()).collect())
        .unwrap_or_else(|| {
            DEFAULT_ACCEPTED_FORMATS
                .iter()
                .map(|s| s.to_string())
                .collect()
        });

    let max_file_size = config
        .get("maxFileSize")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_MAX_FILE_SIZE_MB)
        * 1024
        * 1024;

    let autoplay = config
        .get("autoplay")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let loop_playback = config
        .get("loop")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let show_waveform = config
        .get("showWaveform")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Resolve URL: config.url takes precedence, then source input port
    let url = config
        .get("url")
        .and_then(|v| v.as_str())
        .or_else(|| {
            inputs.get("source").and_then(|m| {
                if let Message::String(s) = m {
                    Some(s.as_str())
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| anyhow::anyhow!("No audio URL configured"))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS))
        .build()?;

    // HEAD request to validate without downloading the full file
    let head_response = client
        .head(url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to reach audio at {}: {}", url, e))?;

    if !head_response.status().is_success() {
        return Ok(error_output(format!(
            "Audio fetch failed with status {} for {}",
            head_response.status(),
            url
        )));
    }

    let content_type = head_response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let base_content_type = content_type.split(';').next().unwrap_or("").trim();
    if !accepted_formats.iter().any(|f| f == base_content_type) {
        return Ok(error_output(format!(
            "Unsupported audio format: {}. Accepted: {}",
            base_content_type,
            accepted_formats.join(", ")
        )));
    }

    let content_length = head_response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    if let Some(size) = content_length {
        if size > max_file_size {
            return Ok(error_output(format!(
                "Audio exceeds maximum file size: {} bytes (max: {} bytes)",
                size, max_file_size
            )));
        }
    }

    let playback_opts = json!({
        "autoplay": autoplay,
        "loop": loop_playback,
        "showWaveform": show_waveform,
    });

    let metadata = json!({
        "contentType": content_type,
        "size": content_length,
        "url": url,
        "playback": playback_opts,
    });

    let mut output = HashMap::new();
    output.insert(
        "audioData".to_string(),
        Message::object(EncodableValue::from(json!({
            "url": url,
            "contentType": content_type,
            "size": content_length,
            "playback": playback_opts,
        }))),
    );
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
