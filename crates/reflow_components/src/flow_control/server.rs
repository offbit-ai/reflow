//! Server request/response actors for webhook-triggered workflows.
//!
//! ServerRequest is an entry point — it receives the incoming webhook
//! data from the ZIP session via IIP and outputs structured fields.
//! ServerResponse constructs the reply to send back.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

// ── Server Request (webhook entry point) ────────────────────────

/// Entry point for webhook-triggered workflows.
///
/// The ZIP session sends the incoming HTTP request as an IIP on
/// the `trigger` port when a webhook hits the configured path.
/// This actor extracts body, headers, params, method, and URL
/// from the request data and outputs them on separate ports.
///
/// Config: path (webhook route), method (HTTP method filter).
#[actor(
    ServerRequestActor,
    inports::<10>(trigger),
    outports::<50>(body, headers, params, method, url),
    state(MemoryState)
)]
pub async fn server_request_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    // The request arrives as the IIP data on the trigger port.
    // The ZIP session populates this with the incoming HTTP request object.
    let request = match payload.get("trigger") {
        Some(msg @ Message::Object(_)) => msg,
        Some(msg @ Message::String(_)) => msg,
        _ => {
            // No request data — output defaults from config
            let path = config.get("path").and_then(|v| v.as_str()).unwrap_or("/webhook");
            let method = config.get("method").and_then(|v| v.as_str()).unwrap_or("POST");
            let mut out = HashMap::new();
            out.insert("body".to_string(), Message::object(EncodableValue::from(json!({}))));
            out.insert("method".to_string(), Message::String(method.to_string().into()));
            out.insert("url".to_string(), Message::String(path.to_string().into()));
            return Ok(out);
        }
    };

    let mut out = HashMap::new();

    if let Message::Object(obj) = request {
        let val: serde_json::Value = obj.as_ref().clone().into();

        if let Some(body) = val.get("body") {
            out.insert("body".to_string(), Message::object(EncodableValue::from(body.clone())));
        }
        if let Some(headers) = val.get("headers") {
            out.insert("headers".to_string(), Message::object(EncodableValue::from(headers.clone())));
        }
        if let Some(params) = val.get("params").or(val.get("query")) {
            out.insert("params".to_string(), Message::object(EncodableValue::from(params.clone())));
        }
        if let Some(method) = val.get("method").and_then(|v| v.as_str()) {
            out.insert("method".to_string(), Message::String(method.to_string().into()));
        }
        if let Some(url) = val.get("url").or(val.get("path")).and_then(|v| v.as_str()) {
            out.insert("url".to_string(), Message::String(url.to_string().into()));
        }
    } else {
        // String trigger — output as body
        out.insert("body".to_string(), request.clone());
    }

    // Fill defaults from config if not present in request
    let path = config.get("path").and_then(|v| v.as_str()).unwrap_or("/webhook");
    let method = config.get("method").and_then(|v| v.as_str()).unwrap_or("POST");

    out.entry("url".to_string())
        .or_insert_with(|| Message::String(path.to_string().into()));
    out.entry("method".to_string())
        .or_insert_with(|| Message::String(method.to_string().into()));

    Ok(out)
}

// ── Server Response (webhook exit point) ────────────────────────

/// Constructs an HTTP response to send back to the webhook caller.
/// Takes body, status code, and headers as inputs.
#[actor(
    ServerResponseActor,
    inports::<10>(body, status, headers),
    outports::<1>(response),
    state(MemoryState)
)]
pub async fn server_response_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let status = match payload.get("status") {
        Some(Message::Integer(v)) => *v as u16,
        _ => config.get("statusCode").and_then(|v| v.as_u64()).unwrap_or(200) as u16,
    };

    let content_type = config
        .get("contentType")
        .and_then(|v| v.as_str())
        .unwrap_or("application/json");

    let body = payload.get("body").cloned().unwrap_or(Message::Flow);

    let body_json = match &body {
        Message::String(s) => json!(s.as_ref()),
        Message::Object(o) => { let v: serde_json::Value = o.as_ref().clone().into(); v }
        Message::Integer(i) => json!(i),
        Message::Float(f) => json!(f),
        Message::Boolean(b) => json!(b),
        _ => json!(null),
    };

    let response = json!({
        "statusCode": status,
        "contentType": content_type,
        "body": body_json,
    });

    Ok([("response".to_string(), Message::object(EncodableValue::from(response)))].into())
}
