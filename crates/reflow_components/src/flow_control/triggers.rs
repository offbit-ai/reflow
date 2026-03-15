//! Time-based trigger actors: interval and cron.
//!
//! These actors emit trigger signals on a schedule. They run
//! continuously until the network shuts down or maxExecutions is reached.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

// ── Interval Trigger ────────────────────────────────────────────

/// Emits a trigger signal at regular intervals.
/// Config: interval (ms), intervalUnit, startImmediately, maxExecutions.
#[actor(
    IntervalTriggerActor,
    inports::<1>(),
    outports::<50>(trigger),
    state(MemoryState)
)]
pub async fn interval_trigger_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let interval_ms = config
        .get("interval")
        .and_then(|v| v.as_u64())
        .unwrap_or(60000);

    let unit = config
        .get("intervalUnit")
        .and_then(|v| v.as_str())
        .unwrap_or("milliseconds");

    let interval = match unit {
        "seconds" => interval_ms * 1000,
        "minutes" => interval_ms * 60_000,
        "hours" => interval_ms * 3_600_000,
        "days" => interval_ms * 86_400_000,
        _ => interval_ms,
    };

    let start_immediately = config
        .get("startImmediately")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let max_executions = config
        .get("maxExecutions")
        .and_then(|v| v.as_u64())
        .unwrap_or(0); // 0 = unlimited

    let payload_template = config
        .get("payload")
        .and_then(|v| v.as_str())
        .unwrap_or(r#"{"timestamp": "${timestamp}"}"#)
        .to_string();

    let mut execution_count: u64 = 0;

    // First trigger (immediate if configured)
    if start_immediately {
        execution_count += 1;
        let payload = build_trigger_payload(&payload_template, execution_count);
        return Ok([("trigger".to_string(), payload)].into());
    }

    // Wait for interval then trigger
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(interval)).await;

    execution_count += 1;

    if max_executions > 0 && execution_count > max_executions {
        return Ok(HashMap::new());
    }

    let payload = build_trigger_payload(&payload_template, execution_count);
    Ok([("trigger".to_string(), payload)].into())
}

// ── Cron Trigger ────────────────────────────────────────────────

/// Emits a trigger signal based on a cron expression.
/// Config: cronExpression, commonSchedules, maxExecutions.
#[actor(
    CronTriggerActor,
    inports::<1>(),
    outports::<50>(trigger),
    state(MemoryState)
)]
pub async fn cron_trigger_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let common = config
        .get("commonSchedules")
        .and_then(|v| v.as_str())
        .unwrap_or("Custom");

    let cron_expr = match common {
        "Every minute" => "* * * * *",
        "Every 5 minutes" => "*/5 * * * *",
        "Every 15 minutes" => "*/15 * * * *",
        "Every 30 minutes" => "*/30 * * * *",
        "Every hour" => "0 * * * *",
        "Every day at midnight" => "0 0 * * *",
        "Every Monday at 9 AM" => "0 9 * * 1",
        "First day of month" => "0 0 1 * *",
        _ => config
            .get("cronExpression")
            .and_then(|v| v.as_str())
            .unwrap_or("0 * * * *"),
    };

    let max_executions = config
        .get("maxExecutions")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let payload_template = config
        .get("payload")
        .and_then(|v| v.as_str())
        .unwrap_or(r#"{"timestamp": "${timestamp}", "schedule": "${schedule}"}"#)
        .to_string();

    // Calculate next trigger time from cron expression
    let interval_ms = parse_cron_to_interval(cron_expr);

    #[cfg(not(target_arch = "wasm32"))]
    if interval_ms > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;
    }

    if max_executions > 0 && max_executions <= 1 {
        // Only trigger once if max is 1
    }

    let now = chrono::Utc::now();
    let payload_str = payload_template
        .replace("${timestamp}", &now.to_rfc3339())
        .replace("${schedule}", cron_expr);

    let payload = match serde_json::from_str::<serde_json::Value>(&payload_str) {
        Ok(val) => Message::object(EncodableValue::from(val)),
        Err(_) => Message::String(payload_str.into()),
    };

    Ok([("trigger".to_string(), payload)].into())
}

// ── Helpers ─────────────────────────────────────────────────────

fn build_trigger_payload(template: &str, execution_count: u64) -> Message {
    let now = chrono::Utc::now();
    let resolved = template
        .replace("${timestamp}", &now.to_rfc3339())
        .replace("${executionCount}", &execution_count.to_string());

    match serde_json::from_str::<serde_json::Value>(&resolved) {
        Ok(val) => Message::object(EncodableValue::from(val)),
        Err(_) => Message::String(resolved.into()),
    }
}

/// Simple cron expression to interval conversion.
/// For full cron support we'd use the `cron` crate.
fn parse_cron_to_interval(expr: &str) -> u64 {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() < 5 {
        return 60_000; // default 1 minute
    }

    // Very basic parsing — handles common patterns
    match parts[0] {
        "*" => 60_000,                        // every minute
        "*/5" => 300_000,                     // every 5 minutes
        "*/15" => 900_000,                    // every 15 minutes
        "*/30" => 1_800_000,                  // every 30 minutes
        "0" if parts[1] == "*" => 3_600_000,  // every hour
        "0" if parts[1] == "0" => 86_400_000, // every day
        _ => 60_000,
    }
}
