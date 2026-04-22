//! Adjusts brightness, contrast, and saturation of an RGBA image stream.
//!
//! Each Data frame is a row of RGBA pixels. Output is the same format
//! with adjusted values. Uses SIMD-accelerated path when available.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{
    stream::{spawn_stream_task, stream_transform},
    ActorContext,
};
use std::collections::HashMap;

#[actor(
    BrightnessContrastActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn brightness_contrast_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    // Factor: 1.0 = no change, >1.0 = brighter, <1.0 = darker
    let brightness = config
        .get("brightness")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;

    // Factor: 1.0 = no change, >1.0 = more contrast, <1.0 = less
    let contrast = config
        .get("contrast")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;

    // Factor: 1.0 = no change, 0.0 = grayscale, >1.0 = vivid
    let saturation = config
        .get("saturation")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;

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
            let mut out = data.to_vec();
            #[cfg(feature = "av-core")]
            {
                if (brightness - 1.0).abs() > f32::EPSILON {
                    reflow_pixel::color::row_brightness(&mut out, brightness);
                }
                if (contrast - 1.0).abs() > f32::EPSILON {
                    reflow_pixel::color::row_contrast(&mut out, contrast);
                }
                if (saturation - 1.0).abs() > f32::EPSILON {
                    reflow_pixel::color::row_saturation(&mut out, saturation);
                }
            }
            #[cfg(not(feature = "av-core"))]
            {
                // Brightness-only fallback
                for px in out.chunks_exact_mut(4) {
                    px[0] = (px[0] as f32 * brightness).clamp(0.0, 255.0) as u8;
                    px[1] = (px[1] as f32 * brightness).clamp(0.0, 255.0) as u8;
                    px[2] = (px[2] as f32 * brightness).clamp(0.0, 255.0) as u8;
                }
                let _ = (contrast, saturation);
            }
            out
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
