//! Keyboard input actor.
//!
//! Outputs key event data when triggered by the runtime.
//! Config (injected by runtime on each event):
//!   key, code, type (keydown/keyup/keypress),
//!   altKey, ctrlKey, shiftKey, metaKey, repeat

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    KeyboardInputActor,
    inports::<1>(),
    outports::<50>(event, key, code, modifiers),
    state(MemoryState)
)]
pub async fn keyboard_input_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let key = config
        .get("key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let code = config
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let event_type = config
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("keydown")
        .to_string();
    let alt = config.get("altKey").and_then(|v| v.as_bool()).unwrap_or(false);
    let ctrl = config
        .get("ctrlKey")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let shift = config
        .get("shiftKey")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let meta = config
        .get("metaKey")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let repeat = config
        .get("repeat")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut out = HashMap::new();
    out.insert(
        "event".to_string(),
        Message::object(EncodableValue::from(json!({
            "type": event_type,
            "key": key,
            "code": code,
            "altKey": alt,
            "ctrlKey": ctrl,
            "shiftKey": shift,
            "metaKey": meta,
            "repeat": repeat,
        }))),
    );
    out.insert("key".to_string(), Message::String(key.into()));
    out.insert("code".to_string(), Message::String(code.into()));
    out.insert(
        "modifiers".to_string(),
        Message::object(EncodableValue::from(json!({
            "alt": alt, "ctrl": ctrl, "shift": shift, "meta": meta,
        }))),
    );
    Ok(out)
}
