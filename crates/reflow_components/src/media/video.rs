//! Video input actor — fetches, validates, and extracts metadata from video.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

const DEFAULT_ACCEPTED_FORMATS: &[&str] = &["video/mp4", "video/webm", "video/ogg"];
const DEFAULT_MAX_FILE_SIZE_MB: u64 = 100;
const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// Video Input Actor — compatible with `tpl_video_input`
///
/// Supports source modes: `url`, `upload`, `stream`, `youtube`, `vimeo`.
/// All modes resolve to a URL — Zeal handles uploads to S3.
/// For direct URLs, validates format/size via HEAD request.
/// For streaming sources (HLS/DASH), validates the manifest URL.
/// For YouTube/Vimeo, resolves the embed URL.
#[actor(
    VideoInputActor,
    inports::<100>(source),
    outports::<50>(videoData, metadata, error),
    state(MemoryState)
)]
pub async fn video_input_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let inputs = context.get_payload();
    let config = context.get_config_hashmap();
    let property_values = config.get("propertyValues").and_then(|v| v.as_object());

    let source_type = property_values
        .and_then(|pv| pv.get("source"))
        .and_then(|v| v.as_str())
        .unwrap_or("url");

    let accepted_formats: Vec<String> = property_values
        .and_then(|pv| pv.get("acceptedFormats"))
        .and_then(|v| v.as_str())
        .map(|s| s.split(',').map(|f| f.trim().to_string()).collect())
        .unwrap_or_else(|| {
            DEFAULT_ACCEPTED_FORMATS
                .iter()
                .map(|s| s.to_string())
                .collect()
        });

    let max_file_size = property_values
        .and_then(|pv| pv.get("maxFileSize"))
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_MAX_FILE_SIZE_MB)
        * 1024
        * 1024;

    let autoplay = property_values
        .and_then(|pv| pv.get("autoplay"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let loop_playback = property_values
        .and_then(|pv| pv.get("loop"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let muted = property_values
        .and_then(|pv| pv.get("muted"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let show_controls = property_values
        .and_then(|pv| pv.get("showControls"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let stream_type = property_values
        .and_then(|pv| pv.get("streamType"))
        .and_then(|v| v.as_str())
        .unwrap_or("auto");

    let playback_opts = json!({
        "autoplay": autoplay,
        "loop": loop_playback,
        "muted": muted,
        "showControls": show_controls,
    });

    let mut output = HashMap::new();

    match source_type {
        "url" => {
            let url = get_url(property_values, inputs)?;

            let client = reqwest::Client::builder()
                .timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS))
                .build()?;

            // HEAD request first to check content-type and size without downloading
            let head_response = client
                .head(url)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to reach video at {}: {}", url, e))?;

            if !head_response.status().is_success() {
                return Ok(error_output(format!(
                    "Video fetch failed with status {} for {}",
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
                    "Unsupported video format: {}. Accepted: {}",
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
                        "Video exceeds maximum file size: {} bytes (max: {} bytes)",
                        size, max_file_size
                    )));
                }
            }

            let metadata = json!({
                "contentType": content_type,
                "size": content_length,
                "url": url,
                "source": "url",
                "playback": playback_opts,
            });

            // For video, output the URL for streaming rather than downloading the full file
            output.insert(
                "videoData".to_string(),
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
        }
        "stream" => {
            let url = get_url(property_values, inputs)?;

            let resolved_stream_type = if stream_type == "auto" {
                detect_stream_type(url)
            } else {
                stream_type.to_string()
            };

            // Validate that the manifest is reachable
            let client = reqwest::Client::builder()
                .timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS))
                .build()?;

            let head_response = client.head(url).send().await;
            let reachable = head_response
                .map(|r| r.status().is_success())
                .unwrap_or(false);

            let metadata = json!({
                "url": url,
                "source": "stream",
                "streamType": resolved_stream_type,
                "reachable": reachable,
                "playback": playback_opts,
            });

            output.insert(
                "videoData".to_string(),
                Message::object(EncodableValue::from(json!({
                    "url": url,
                    "streamType": resolved_stream_type,
                    "playback": playback_opts,
                }))),
            );
            output.insert(
                "metadata".to_string(),
                Message::object(EncodableValue::from(metadata)),
            );
        }
        "youtube" => {
            let url = get_url(property_values, inputs)?;
            let video_id = extract_youtube_id(url);

            let embed_url = video_id
                .as_ref()
                .map(|id| format!("https://www.youtube.com/embed/{}", id))
                .unwrap_or_else(|| url.to_string());

            let metadata = json!({
                "source": "youtube",
                "url": url,
                "videoId": video_id,
                "embedUrl": embed_url,
                "playback": playback_opts,
            });

            output.insert(
                "videoData".to_string(),
                Message::object(EncodableValue::from(json!({
                    "embedUrl": embed_url,
                    "videoId": video_id,
                    "platform": "youtube",
                    "playback": playback_opts,
                }))),
            );
            output.insert(
                "metadata".to_string(),
                Message::object(EncodableValue::from(metadata)),
            );
        }
        "vimeo" => {
            let url = get_url(property_values, inputs)?;
            let video_id = extract_vimeo_id(url);

            let embed_url = video_id
                .as_ref()
                .map(|id| format!("https://player.vimeo.com/video/{}", id))
                .unwrap_or_else(|| url.to_string());

            let metadata = json!({
                "source": "vimeo",
                "url": url,
                "videoId": video_id,
                "embedUrl": embed_url,
                "playback": playback_opts,
            });

            output.insert(
                "videoData".to_string(),
                Message::object(EncodableValue::from(json!({
                    "embedUrl": embed_url,
                    "videoId": video_id,
                    "platform": "vimeo",
                    "playback": playback_opts,
                }))),
            );
            output.insert(
                "metadata".to_string(),
                Message::object(EncodableValue::from(metadata)),
            );
        }
        // upload resolves to url — Zeal mutates the source property to "url"
        // after uploading to S3, so by the time the graph reaches Reflow,
        // uploads are already URLs. Treat the same as "url".
        "upload" => {
            let url = get_url(property_values, inputs)?;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS))
                .build()?;

            let head_response = client
                .head(url)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to reach video at {}: {}", url, e))?;

            let content_type = head_response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("video/mp4")
                .to_string();

            let content_length = head_response
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());

            let metadata = json!({
                "contentType": content_type,
                "size": content_length,
                "url": url,
                "source": "upload",
                "playback": playback_opts,
            });

            output.insert(
                "videoData".to_string(),
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
        }
        other => {
            return Ok(error_output(format!("Unsupported source type: {}", other)));
        }
    }

    Ok(output)
}

fn get_url<'a>(
    property_values: Option<&'a serde_json::Map<String, serde_json::Value>>,
    inputs: &'a HashMap<String, Message>,
) -> Result<&'a str> {
    property_values
        .and_then(|pv| pv.get("url"))
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
        .ok_or_else(|| anyhow::anyhow!("No video URL configured"))
}

fn error_output(msg: String) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.into()));
    out
}

fn detect_stream_type(url: &str) -> String {
    let lower = url.to_lowercase();
    if lower.contains(".m3u8") {
        "hls".to_string()
    } else if lower.contains(".mpd") {
        "dash".to_string()
    } else {
        "auto".to_string()
    }
}

/// Extract YouTube video ID from various URL formats.
fn extract_youtube_id(url: &str) -> Option<String> {
    // youtube.com/watch?v=ID
    if let Some(pos) = url.find("v=") {
        let rest = &url[pos + 2..];
        let id: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !id.is_empty() {
            return Some(id);
        }
    }
    // youtu.be/ID
    if url.contains("youtu.be/") {
        if let Some(pos) = url.find("youtu.be/") {
            let rest = &url[pos + 9..];
            let id: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !id.is_empty() {
                return Some(id);
            }
        }
    }
    // youtube.com/embed/ID
    if let Some(pos) = url.find("/embed/") {
        let rest = &url[pos + 7..];
        let id: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !id.is_empty() {
            return Some(id);
        }
    }
    None
}

/// Extract Vimeo video ID from URL.
fn extract_vimeo_id(url: &str) -> Option<String> {
    // vimeo.com/123456
    let stripped = url.trim_end_matches('/');
    let last_segment = stripped.rsplit('/').next()?;
    if last_segment.chars().all(|c| c.is_ascii_digit()) && !last_segment.is_empty() {
        Some(last_segment.to_string())
    } else {
        None
    }
}
