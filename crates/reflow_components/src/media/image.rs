//! Image input actor — fetches, validates, and extracts metadata from images.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

const DEFAULT_ACCEPTED_FORMATS: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];
const DEFAULT_MAX_FILE_SIZE_MB: u64 = 10;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Image Input Actor — compatible with `tpl_image_input`
///
/// All source modes (url, upload, base64) resolve to a URL —
/// Zeal handles uploads to S3 and provides the resulting URL.
/// The actor fetches the image, validates format/size, extracts
/// dimensions from the binary header, and outputs image data + metadata.
#[actor(
    ImageInputActor,
    inports::<100>(source),
    outports::<50>(imageData, metadata, error),
    state(MemoryState)
)]
pub async fn image_input_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
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

    let display_mode = config
        .get("displayMode")
        .and_then(|v| v.as_str())
        .unwrap_or("contain");

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
        .ok_or_else(|| anyhow::anyhow!("No image URL configured"))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS))
        .build()?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch image from {}: {}", url, e))?;

    let status = response.status();
    if !status.is_success() {
        return Ok(error_output(format!(
            "Image fetch failed with status {} for {}",
            status, url
        )));
    }

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let base_content_type = content_type.split(';').next().unwrap_or("").trim();
    if !accepted_formats.iter().any(|f| f == base_content_type) {
        return Ok(error_output(format!(
            "Unsupported image format: {}. Accepted: {}",
            base_content_type,
            accepted_formats.join(", ")
        )));
    }

    let content_length = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    if let Some(size) = content_length {
        if size > max_file_size {
            return Ok(error_output(format!(
                "Image exceeds maximum file size: {} bytes (max: {} bytes)",
                size, max_file_size
            )));
        }
    }

    let bytes = response.bytes().await?;
    let size = bytes.len() as u64;

    if size > max_file_size {
        return Ok(error_output(format!(
            "Image exceeds maximum file size: {} bytes (max: {} bytes)",
            size, max_file_size
        )));
    }

    let dimensions = extract_image_dimensions(&bytes);

    let metadata = json!({
        "contentType": content_type,
        "size": size,
        "url": url,
        "displayMode": display_mode,
        "width": dimensions.map(|(w, _)| w),
        "height": dimensions.map(|(_, h)| h),
    });

    let mut output = HashMap::new();
    output.insert(
        "imageData".to_string(),
        Message::object(EncodableValue::from(json!({
            "url": url,
            "contentType": content_type,
            "size": size,
            "displayMode": display_mode,
            "width": dimensions.map(|(w, _)| w),
            "height": dimensions.map(|(_, h)| h),
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

/// Extract width/height from common image formats by reading the header bytes.
fn extract_image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 {
        return None;
    }

    // PNG: bytes 16-23 contain width (4 bytes BE) and height (4 bytes BE) in IHDR
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return Some((w, h));
    }

    // GIF: bytes 6-9 contain width (2 bytes LE) and height (2 bytes LE)
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        let w = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let h = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        return Some((w, h));
    }

    // JPEG: scan for SOF0/SOF2 markers (0xFF 0xC0 or 0xFF 0xC2)
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            if marker == 0xC0 || marker == 0xC2 {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                return Some((w, h));
            }
            if i + 3 < bytes.len() {
                let seg_len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
                i += 2 + seg_len;
            } else {
                break;
            }
        }
    }

    // WebP: RIFF header, "WEBP" at offset 8
    if bytes.len() > 30 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        if &bytes[12..16] == b"VP8 " && bytes.len() > 29 {
            let w = u16::from_le_bytes([bytes[26], bytes[27]]) as u32 & 0x3FFF;
            let h = u16::from_le_bytes([bytes[28], bytes[29]]) as u32 & 0x3FFF;
            return Some((w, h));
        }
        if &bytes[12..16] == b"VP8L" && bytes.len() > 24 {
            let bits = u32::from_le_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]);
            let w = (bits & 0x3FFF) + 1;
            let h = ((bits >> 14) & 0x3FFF) + 1;
            return Some((w, h));
        }
    }

    None
}
