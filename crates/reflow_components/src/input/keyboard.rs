//! Keyboard input actor.
//!
//! Outputs key event data when triggered by the runtime.
//! The runtime sends event data as Message::Object on any port.
//! The actor extracts key, code, modifiers from the incoming message.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    KeyboardInputActor,
    inports::<1>(),
    outports::<50>(event, key, code, modifiers),
    state(MemoryState)
)]
pub async fn keyboard_input_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let event_data = super::extract_event_data(&ctx);

    let key = event_data
        .get("key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let code = event_data
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let event_type = event_data
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("keydown")
        .to_string();
    let alt = event_data
        .get("altKey")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let ctrl = event_data
        .get("ctrlKey")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let shift = event_data
        .get("shiftKey")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let meta = event_data
        .get("metaKey")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let repeat = event_data
        .get("repeat")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut out = HashMap::new();
    out.insert(
        "event".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": event_type, "key": key, "code": code,
            "altKey": alt, "ctrlKey": ctrl, "shiftKey": shift,
            "metaKey": meta, "repeat": repeat,
        }))),
    );
    out.insert("key".to_string(), Message::String(key.into()));
    out.insert("code".to_string(), Message::String(code.into()));
    out.insert(
        "modifiers".to_string(),
        Message::object(EncodableValue::from(
            json!({ "alt": alt, "ctrl": ctrl, "shift": shift, "meta": meta }),
        )),
    );
    Ok(out)
}
