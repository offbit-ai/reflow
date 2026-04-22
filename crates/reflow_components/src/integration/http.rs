//! HTTP request actor with real reqwest implementation.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

/// HTTP Request Actor - Compatible with tpl_http_request
///
/// Executes HTTP requests using reqwest.
#[actor(
    HttpRequestActor,
    inports::<100>(data_in, headers_in),
    outports::<50>(response_out, error_out),
    state(MemoryState)
)]
pub async fn http_request_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let inputs = context.get_payload();
    let config = context.get_config_hashmap();

    let url = config
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("URL not configured in Zeal node"))?;

    let method = config
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET");

    let timeout_ms = config
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);

    // Build the request
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()?;

    let mut request_builder = match method.to_uppercase().as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "PATCH" => client.patch(url),
        "DELETE" => client.delete(url),
        "HEAD" => client.head(url),
        other => return Err(anyhow::anyhow!("Unsupported HTTP method: {}", other)),
    };

    // Apply custom headers from headers_in port
    if let Some(Message::Object(headers_obj)) = inputs.get("headers_in") {
        if let Ok(Value::Object(map)) = serde_json::to_value(headers_obj) {
            for (key, value) in map {
                if let Some(v) = value.as_str() {
                    request_builder = request_builder.header(&key, v);
                }
            }
        }
    }

    // Apply headers from config
    if let Some(headers) = config.get("headers").and_then(|v| v.as_object()) {
        for (key, value) in headers {
            if let Some(v) = value.as_str() {
                request_builder = request_builder.header(key.as_str(), v);
            }
        }
    }

    // Apply body from data_in port
    if let Some(body_data) = inputs.get("data_in") {
        let body_json = serde_json::to_value(body_data)?;
        request_builder = request_builder.json(&body_json);
    }

    // Apply body from config
    if let Some(body) = config.get("body") {
        request_builder = request_builder.json(body);
    }

    let mut output_messages = HashMap::new();

    match request_builder.send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let headers: HashMap<String, String> = response
                .headers()
                .iter()
                .filter_map(|(k, v)| v.to_str().ok().map(|val| (k.to_string(), val.to_string())))
                .collect();

            let body = response.text().await.unwrap_or_default();

            // Try to parse body as JSON, fall back to string
            let body_value: Value = serde_json::from_str(&body).unwrap_or(Value::String(body));

            output_messages.insert(
                "response_out".to_string(),
                Message::object(EncodableValue::from(json!({
                    "status": status,
                    "headers": headers,
                    "body": body_value,
                    "url": url,
                    "method": method,
                }))),
            );
        }
        Err(e) => {
            output_messages.insert(
                "error_out".to_string(),
                Message::Error(
                    format!(
                        "HTTP request failed: {} (url: {}, method: {})",
                        e, url, method
                    )
                    .into(),
                ),
            );
        }
    }

    Ok(output_messages)
}
