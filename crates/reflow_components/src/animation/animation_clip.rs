//! Animation clip actor — defines keyframed animation tracks per bone.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    AnimationClipActor,
    inports::<10>(clip_data),
    outports::<1>(clip, metadata),
    state(MemoryState)
)]
pub async fn animation_clip_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let name = config
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("clip")
        .to_string();

    let duration = config
        .get("duration")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);

    // Channels from config or inport
    let channels = if let Some(Message::Object(obj)) = payload.get("clip_data") {
        let v: Value = obj.as_ref().clone().into();
        v.get("channels").cloned().unwrap_or_else(|| json!([]))
    } else {
        config.get("channels").cloned().unwrap_or_else(|| json!([]))
    };

    let channel_list = channels.as_array().cloned().unwrap_or_default();

    // Validate channels
    let mut validated: Vec<Value> = Vec::new();
    for ch in &channel_list {
        let bone_index = ch.get("boneIndex").and_then(|v| v.as_u64()).unwrap_or(0);
        let property = ch
            .get("property")
            .and_then(|v| v.as_str())
            .unwrap_or("rotation")
            .to_string();
        let interpolation = ch
            .get("interpolation")
            .and_then(|v| v.as_str())
            .unwrap_or("linear")
            .to_string();
        let times = ch.get("times").cloned().unwrap_or_else(|| json!([]));
        let values = ch.get("values").cloned().unwrap_or_else(|| json!([]));

        validated.push(json!({
            "boneIndex": bone_index,
            "property": property,
            "interpolation": interpolation,
            "times": times,
            "values": values,
        }));
    }

    let clip = json!({
        "name": name,
        "duration": duration,
        "channelCount": validated.len(),
        "channels": validated,
    });

    let mut out = HashMap::new();
    out.insert(
        "clip".to_string(),
        Message::object(EncodableValue::from(clip)),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "name": name,
            "duration": duration,
            "channelCount": validated.len(),
        }))),
    );
    Ok(out)
}
