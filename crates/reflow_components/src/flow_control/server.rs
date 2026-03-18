//! Server request/response actors for webhook-triggered workflows.
//!
//! ServerRequest is a source actor — no inports. The network/server
//! invokes it directly when a webhook arrives, with the HTTP request
//! data in the actor's config. It outputs structured fields.
//!
//! ServerResponse constructs the reply to send back.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

// ── Server Request (webhook entry point) ────────────────────────

/// Entry point for webhook-triggered workflows.
///
/// No inports — the server calls this actor directly via
/// `network.execute_actor()` when a webhook hits the configured path.
/// The HTTP request data is placed in the actor's config by the server.
///
/// Config (set by server at invocation):
///   - body: request body (JSON)
///   - headers: request headers
///   - query: query parameters
///   - method: HTTP method
///   - path: request path
///
/// Config (set by user in template):
///   - path: webhook route to listen on
///   - method: HTTP method filter
#[actor(
    ServerRequestActor,
    inports::<1>(),
    outports::<50>(body, headers, params, method, url),
    state(MemoryState)
)]
pub async fn server_request_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let mut out = HashMap::new();

    // Body — the main payload
    if let Some(body) = config.get("body") {
        out.insert(
            "body".to_string(),
            Message::object(EncodableValue::from(body.clone())),
        );
    } else {
        out.insert(
            "body".to_string(),
            Message::object(EncodableValue::from(json!({}))),
        );
    }

    // Headers
    if let Some(headers) = config.get("headers") {
        out.insert(
            "headers".to_string(),
            Message::object(EncodableValue::from(headers.clone())),
        );
    }

    // Query params
    if let Some(params) = config.get("query").or(config.get("params")) {
        out.insert(
            "params".to_string(),
            Message::object(EncodableValue::from(params.clone())),
        );
    }

    // Method
    let method = config
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("POST");
    out.insert(
        "method".to_string(),
        Message::String(method.to_string().into()),
    );

    // URL/path
    let path = config
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("/webhook");
    out.insert("url".to_string(), Message::String(path.to_string().into()));

    Ok(out)
}

// ── Server Response (webhook exit point) ────────────────────────

/// Constructs an HTTP response to send back to the webhook caller.
#[actor(
    ServerResponseActor,
    inports::<10>(body, status, headers),
    outports::<1>(response),
    state(MemoryState)
)]
pub async fn server_response_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let status = match payload.get("status") {
        Some(Message::Integer(v)) => *v as u16,
        _ => config
            .get("statusCode")
            .and_then(|v| v.as_u64())
            .unwrap_or(200) as u16,
    };

    let content_type = config
        .get("contentType")
        .and_then(|v| v.as_str())
        .unwrap_or("application/json");

    let body = payload
        .get("body")
        .cloned()
        .unwrap_or(Message::object(EncodableValue::from(json!({}))));

    let body_json = match &body {
        Message::String(s) => json!(s.as_ref()),
        Message::Object(o) => {
            let v: serde_json::Value = o.as_ref().clone().into();
            v
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

    Ok([(
        "response".to_string(),
        Message::object(EncodableValue::from(response)),
    )]
    .into())
}
