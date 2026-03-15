//! Server request/response actors for webhook-triggered workflows.
//!
//! These actors represent the entry/exit points of HTTP-triggered workflows.
//! ServerRequest receives incoming webhook data, ServerResponse sends the reply.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;

// ── Server Request (webhook entry point) ────────────────────────

/// Represents an incoming HTTP request that triggers the workflow.
/// The request data (body, headers, params) arrives as the payload.
/// Outputs the request data for downstream processing.
#[actor(
    ServerRequestActor,
    inports::<10>(request),
    outports::<50>(body, headers, params, method, url),
    state(MemoryState)
)]
pub async fn server_request_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    // The request data comes from the network trigger (via ZIP webhook)
    let request = payload.get("request").cloned().unwrap_or(Message::Flow);

    let mut out = HashMap::new();

    // Try to extract structured request fields
    if let Message::Object(obj) = &request {
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
        // Pass through as body
        out.insert("body".to_string(), request);
    }

    // Add configured defaults
    let path = config.get("path").and_then(|v| v.as_str()).unwrap_or("/webhook");
    let method = config.get("method").and_then(|v| v.as_str()).unwrap_or("POST");

    if !out.contains_key("url") {
        out.insert("url".to_string(), Message::String(path.to_string().into()));
    }
    if !out.contains_key("method") {
        out.insert("method".to_string(), Message::String(method.to_string().into()));
    }

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
        Message::Object(o) => {
            let val: serde_json::Value = o.as_ref().clone().into();
            val
        }
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
