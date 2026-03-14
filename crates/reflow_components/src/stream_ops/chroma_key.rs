//! Green/blue screen removal from an RGBA image stream.
//!
//! Each Data frame is a row of RGBA pixels. Pixels matching the key
//! color (within tolerance) have their alpha set to 0.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{
    stream::{spawn_stream_task, stream_transform},
    ActorContext,
};
use std::collections::HashMap;

#[actor(
    ChromaKeyActor,
    inports::<100>(stream),
    outports::<50>(stream, error),
    state(MemoryState)
)]
pub async fn chroma_key_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let key_color = config
        .get("keyColor")
        .and_then(|v| v.as_str())
        .unwrap_or("green")
        .to_string();

    let tolerance = config
        .get("tolerance")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3) as f32;

    let spill = config
        .get("spillSuppression")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5) as f32;

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

    #[cfg(feature = "av-core")]
    let chroma_config = {
        let mut cfg = match key_color.as_str() {
            "blue" => reflow_pixel::chroma::ChromaKeyConfig::blue(),
            _ => reflow_pixel::chroma::ChromaKeyConfig::green(),
        };
        cfg.hue_tolerance = tolerance;
        cfg.spill_suppression = spill;
        cfg
    };

    spawn_stream_task(async move {
        stream_transform(input_rx, tx, move |data: &[u8]| {
            let mut out = data.to_vec();
            #[cfg(feature = "av-core")]
            {
                reflow_pixel::chroma::apply_chroma_key(&mut out, &chroma_config);
            }
            #[cfg(not(feature = "av-core"))]
            {
                // Basic green key fallback
                let _ = (tolerance, spill);
                for px in out.chunks_exact_mut(4) {
                    if px[1] > px[0] && px[1] > px[2] {
                        px[3] = 0;
                    }
                }
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
