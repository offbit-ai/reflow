//! Signal and Subscriber actors — pub/sub within the DAG.
//!
//! **Signal** is a message format: `{ "id": "EVENT_NAME", "data": { ... } }`.
//! Any actor can emit signals (FSM emit, custom actors, etc).
//!
//! **SignalActor** is a trigger-to-signal bridge: receives a trigger from
//! upstream flow, emits a signal with a configured event ID and data.
//!
//! **SubscriberActor** listens for signals matching a configured event ID.
//! When a matching signal arrives, it forwards the data payload. Non-matching
//! signals are silently dropped.
//!
//! ## SignalActor config
//!
//! ```json
//! { "event": "HOVER", "data": { "scale": 1.04 } }
//! ```
//!
//! ## SubscriberActor config
//!
//! ```json
//! { "event": "HOVER" }
//! ```
//!
//! ## DAG wiring
//!
//! ```text
//! HitTest → Signal("HOVER") → event → FSM:event
//!
//! FSM:emit → Subscriber("HOVER") → data → renderer:values
//! FSM:emit → Subscriber("PRESS") → data → timeline:control
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// SignalActor — trigger-to-signal bridge
// ═══════════════════════════════════════════════════════════════════════════

#[actor(
    SignalActor,
    inports::<10>(trigger),
    outports::<50>(signal),
    state(MemoryState)
)]
pub async fn signal_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    if !payload.contains_key("trigger") {
        return Ok(HashMap::new());
    }

    let event_id = config
        .get("event")
        .and_then(|v| v.as_str())
        .unwrap_or("signal");

    // Data: config defaults merged with trigger payload
    let mut data = config
        .get("data")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

    if let Some(Message::Object(obj)) = payload.get("trigger") {
        let trigger_v: Value = obj.as_ref().clone().into();
        if let (Some(base), Some(overlay)) = (data.as_object_mut(), trigger_v.as_object()) {
            for (k, v) in overlay {
                base.insert(k.clone(), v.clone());
            }
        }
    }

    let signal = json!({ "id": event_id, "data": data });

    Ok(HashMap::from([(
        "signal".to_string(),
        Message::object(EncodableValue::from(signal)),
    )]))
}

// ═══════════════════════════════════════════════════════════════════════════
// SubscriberActor — filters signals by event ID, forwards data
// ═══════════════════════════════════════════════════════════════════════════

#[actor(
    SubscriberActor,
    inports::<100>(signal),
    outports::<50>(data, trigger),
    state(MemoryState)
)]
pub async fn subscriber_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let listen_for = config.get("event").and_then(|v| v.as_str()).unwrap_or("*");

    let msg = match payload.get("signal") {
        Some(m) => m,
        None => return Ok(HashMap::new()),
    };

    // Extract signal ID and data from the message
    let (signal_id, signal_data) = match msg {
        Message::Object(obj) => {
            let v: Value = obj.as_ref().clone().into();
            let id = v
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
            let data = v.get("data").cloned().unwrap_or(Value::Null);
            (id, data)
        }
        Message::String(s) => (s.to_string(), Value::Null),
        _ => return Ok(HashMap::new()),
    };

    // Filter: only forward matching event IDs (or "*" for all)
    if listen_for != "*" && signal_id != listen_for {
        return Ok(HashMap::new());
    }

    let mut out = HashMap::new();

    // Forward data payload
    if !signal_data.is_null() {
        out.insert(
            "data".to_string(),
            Message::object(EncodableValue::from(signal_data)),
        );
    }

    // Always emit trigger (Flow) for simple downstream activation
    out.insert("trigger".to_string(), Message::Flow);

    Ok(out)
}
