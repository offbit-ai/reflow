//! Encodes a raw RGBA stream into PNG, JPEG, or WebP bytes.
//!
//! Collects the entire stream (all row data), reconstructs the image
//! buffer, and encodes to the configured format. Outputs Message::Bytes.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, stream::StreamFrame, ActorContext};
use reflow_actor_macro::actor;
use serde_json::json;
use std::collections::HashMap;

#[actor(
    ImageEncodeActor,
    inports::<100>(stream),
    outports::<50>(output, metadata, error),
    state(MemoryState)
)]
pub async fn image_encode_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let format_str = config
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("png")
        .to_string();

    let quality = config.get("quality").and_then(|v| v.as_u64()).unwrap_or(90) as u8;

    // Take the stream receiver
    let rx = match context.take_stream_receiver("stream") {
        Some(rx) => rx,
        None => return Ok(error_output("No StreamHandle on stream port")),
    };

    // Read Begin frame for dimensions
    let first = rx
        .recv_async()
        .await
        .map_err(|_| anyhow::anyhow!("Stream closed before Begin"))?;

    let (mut width, mut height, channels) = match &first {
        StreamFrame::Begin { metadata, .. } => {
            let w = metadata
                .as_ref()
                .and_then(|m| m.get("width"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let h = metadata
                .as_ref()
                .and_then(|m| m.get("height"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let c = metadata
                .as_ref()
                .and_then(|m| m.get("channels"))
                .and_then(|v| v.as_u64())
                .unwrap_or(4) as u32;
            (w, h, c)
        }
        _ => (0, 0, 4),
    };

    // Collect all data frames
    let mut pixel_data: Vec<u8> = Vec::new();
    loop {
        match rx.recv_async().await {
            Ok(StreamFrame::Data(data)) => pixel_data.extend_from_slice(&data),
            Ok(StreamFrame::End) => break,
            Ok(StreamFrame::Error(e)) => return Ok(error_output(&format!("Stream error: {}", e))),
            Ok(StreamFrame::Begin { .. }) => {} // ignore duplicate
            Err(_) => break,
        }
    }

    // Infer dimensions if not provided
    if width == 0 || height == 0 {
        let total_pixels = pixel_data.len() as u32 / channels;
        let side = (total_pixels as f64).sqrt() as u32;
        width = side;
        height = total_pixels / side;
    }

    if width == 0 || height == 0 {
        return Ok(error_output("Cannot determine image dimensions"));
    }

    // Encode
    let encoded = match format_str.as_str() {
        "jpeg" | "jpg" => encode_jpeg(&pixel_data, width, height, channels, quality),
        "webp" => encode_webp(&pixel_data, width, height, channels, quality),
        _ => encode_png(&pixel_data, width, height, channels),
    };

    match encoded {
        Ok(bytes) => {
            let encoded_size = bytes.len();
            let mime = match format_str.as_str() {
                "jpeg" | "jpg" => "image/jpeg",
                "webp" => "image/webp",
                _ => "image/png",
            };
            let mut results = HashMap::new();
            results.insert("output".to_string(), Message::bytes(bytes));
            results.insert(
                "metadata".to_string(),
                Message::object(EncodableValue::from(json!({
                    "format": format_str,
                    "mimeType": mime,
                    "width": width,
                    "height": height,
                    "encodedSize": encoded_size,
                    "rawSize": pixel_data.len(),
                }))),
            );
            Ok(results)
        }
        Err(e) => Ok(error_output(&format!("Encode error: {}", e))),
    }
}

fn encode_png(pixels: &[u8], width: u32, height: u32, channels: u32) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;

    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);

    let color_type = match channels {
        1 => image::ColorType::L8,
        3 => image::ColorType::Rgb8,
        _ => image::ColorType::Rgba8,
    };

    encoder
        .write_image(pixels, width, height, color_type.into())
        .map_err(|e| format!("PNG encode: {}", e))?;

    Ok(buf)
}

fn encode_jpeg(
    pixels: &[u8],
    width: u32,
    height: u32,
    channels: u32,
    quality: u8,
) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;

    // JPEG needs RGB, not RGBA
    let rgb_pixels = if channels == 4 {
        pixels
            .chunks_exact(4)
            .flat_map(|px| [px[0], px[1], px[2]])
            .collect::<Vec<u8>>()
    } else if channels == 1 {
        pixels.iter().flat_map(|&g| [g, g, g]).collect::<Vec<u8>>()
    } else {
        pixels.to_vec()
    };

    let mut buf = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
    encoder
        .write_image(&rgb_pixels, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|e| format!("JPEG encode: {}", e))?;

    Ok(buf)
}

fn encode_webp(
    pixels: &[u8],
    width: u32,
    height: u32,
    channels: u32,
    _quality: u8,
) -> Result<Vec<u8>, String> {
    // image crate's WebP encoder is lossless only
    use image::ImageEncoder;

    let color_type = match channels {
        1 => image::ColorType::L8,
        3 => image::ColorType::Rgb8,
        _ => image::ColorType::Rgba8,
    };

    let mut buf = Vec::new();
    let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut buf);
    encoder
        .write_image(pixels, width, height, color_type.into())
        .map_err(|e| format!("WebP encode: {}", e))?;

    Ok(buf)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
