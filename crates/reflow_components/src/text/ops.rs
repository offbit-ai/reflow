//! Text processing actors: JSON parser, regex matcher, date/time.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

// ── JSON Parser ─────────────────────────────────────────────────

/// Parses a JSON string into a structured Message::Object.
/// Can also extract a value at a JSON path.
#[actor(JsonParserActor, inports::<10>(input), outports::<1>(output, error), state(MemoryState))]
pub async fn json_parser_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();

    let text = match payload.get("input") {
        Some(Message::String(s)) => s.to_string(),
        Some(Message::Bytes(b)) => String::from_utf8_lossy(b).to_string(),
        _ => {
            return Ok(error_output("Expected String or Bytes on input port"));
        }
    };

    let parsed: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            return Ok(error_output(&format!("JSON parse error: {}", e)));
        }
    };

    // Optional path extraction (e.g., "data.users[0].name")
    let result = if let Some(path) = config.get("path").and_then(|v| v.as_str()) {
        extract_json_path(&parsed, path)
    } else {
        parsed
    };

    let mut out = HashMap::new();
    out.insert(
        "output".to_string(),
        Message::object(EncodableValue::from(result)),
    );
    Ok(out)
}

fn extract_json_path(value: &serde_json::Value, path: &str) -> serde_json::Value {
    let mut current = value;
    for key in path.split('.') {
        // Handle array index: "items[0]"
        if let Some(bracket) = key.find('[') {
            let field = &key[..bracket];
            let idx_str = &key[bracket + 1..key.len() - 1];
            if !field.is_empty() {
                current = &current[field];
            }
            if let Ok(idx) = idx_str.parse::<usize>() {
                current = &current[idx];
            }
        } else {
            current = &current[key];
        }
    }
    current.clone()
}

// ── Regex Matcher ───────────────────────────────────────────────

/// Matches a regex pattern against input text.
/// Outputs: matched (bool), matches (array of match strings), groups (named groups).
#[actor(RegexMatcherActor, inports::<10>(input), outports::<1>(matched, matches, groups, error), state(MemoryState))]
pub async fn regex_matcher_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();

    let text = match payload.get("input") {
        Some(Message::String(s)) => s.to_string(),
        _ => return Ok(error_output("Expected String on input port")),
    };

    let pattern = config
        .get("pattern")
        .and_then(|v| v.as_str())
        .unwrap_or(".*");

    let global = config
        .get("global")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Simple regex via std (no external dep)
    // For full regex support, we'd use the `regex` crate.
    // Here we do basic substring matching as a fallback.
    let mut out = HashMap::new();

    // Check if it's a simple literal pattern (no regex metacharacters)
    let is_literal = !pattern.chars().any(|c| {
        matches!(
            c,
            '.' | '*' | '+' | '?' | '[' | ']' | '(' | ')' | '{' | '}' | '|' | '^' | '$' | '\\'
        )
    });

    if is_literal {
        let found = text.contains(pattern);
        out.insert("matched".to_string(), Message::Boolean(found));
        if found {
            let match_list: Vec<serde_json::Value> = if global {
                text.match_indices(pattern)
                    .map(|(i, m)| json!({ "index": i, "match": m }))
                    .collect()
            } else {
                vec![json!(pattern)]
            };
            out.insert(
                "matches".to_string(),
                Message::object(EncodableValue::from(json!(match_list))),
            );
        }
    } else {
        // For complex patterns, report that we matched the whole text against the pattern
        // Full regex support would require the `regex` crate
        out.insert("matched".to_string(), Message::Boolean(false));
        out.insert(
            "error".to_string(),
            Message::Error(
                "Complex regex requires the regex crate — use literal patterns or script actors"
                    .to_string()
                    .into(),
            ),
        );
    }

    Ok(out)
}

// ── Date/Time ───────────────────────────────────────────────────

/// Date/time operations: current time, parse, format, arithmetic.
#[actor(DateTimeActor, inports::<1>(), outports::<1>(output, timestamp, formatted, error), state(MemoryState))]
pub async fn date_time_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let operation = config
        .get("operation")
        .and_then(|v| v.as_str())
        .unwrap_or("now");

    let format_str = config
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("%Y-%m-%dT%H:%M:%S%.3fZ");

    let now = chrono::Utc::now();

    let mut out = HashMap::new();

    match operation {
        "now" => {
            out.insert(
                "timestamp".to_string(),
                Message::Integer(now.timestamp_millis()),
            );
            out.insert(
                "formatted".to_string(),
                Message::String(now.format(format_str).to_string().into()),
            );
            out.insert(
                "output".to_string(),
                Message::object(EncodableValue::from(json!({
                    "timestamp": now.timestamp_millis(),
                    "iso": now.to_rfc3339(),
                    "formatted": now.format(format_str).to_string(),
                    "year": now.format("%Y").to_string(),
                    "month": now.format("%m").to_string(),
                    "day": now.format("%d").to_string(),
                    "hour": now.format("%H").to_string(),
                    "minute": now.format("%M").to_string(),
                    "second": now.format("%S").to_string(),
                }))),
            );
        }
        "epoch" => {
            out.insert("timestamp".to_string(), Message::Integer(now.timestamp()));
        }
        _ => {
            out.insert(
                "error".to_string(),
                Message::Error(format!("Unknown operation: {}", operation).into()),
            );
        }
    }

    Ok(out)
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
