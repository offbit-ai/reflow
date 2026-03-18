//! Explicit stream fan-out — one input stream becomes two output streams.
//!
//! Unlike the implicit observer tap (which drops frames under pressure),
//! StreamTee provides lossless fan-out with full backpressure from both
//! consumers. Use this when both outputs need every frame.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{stream::StreamBroadcaster, ActorContext};
use std::collections::HashMap;

#[actor(
    StreamTeeActor,
    inports::<100>(stream),
    outports::<50>(stream_a, stream_b, error),
    state(MemoryState)
)]
pub async fn stream_tee_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();

    let buffer_size = config
        .get("bufferSize")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(64);

    let input_rx = match context.take_stream_receiver("stream") {
        Some(rx) => rx,
        None => {
            return Ok(error_output("No StreamHandle on stream port"));
        }
    };

    let payload = context.get_payload();
    let input_handle = match payload.get("stream") {
        Some(Message::StreamHandle(h)) => h,
        _ => {
            return Ok(error_output("Expected StreamHandle message"));
        }
    };

    let (tx_a, handle_a) = context.create_stream(
        "stream_a",
        input_handle.content_type.clone(),
        input_handle.size_hint,
        Some(buffer_size),
    );
    let (tx_b, handle_b) = context.create_stream(
        "stream_b",
        input_handle.content_type.clone(),
        input_handle.size_hint,
        Some(buffer_size),
    );

    StreamBroadcaster::spawn(input_rx, vec![tx_a, tx_b]);

    let mut results = HashMap::new();
    results.insert("stream_a".to_string(), Message::stream_handle(handle_a));
    results.insert("stream_b".to_string(), Message::stream_handle(handle_b));
    Ok(results)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
