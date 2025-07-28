//! Example Reflow actor plugin that multiplies input by a configurable factor

use reflow_wasm::*;
use std::collections::HashMap;

/// Define the plugin metadata
fn metadata() -> PluginMetadata {
    PluginMetadata {
        component: "MultiplyActor".to_string(),
        description: "Multiplies numeric input by a configurable factor".to_string(),
        inports: vec![
            port_def!("value", "Number to multiply", "Integer", required),
        ],
        outports: vec![
            port_def!("result", "Multiplied result", "Integer"),
            port_def!("error", "Error output", "String"),
        ],
        config_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "factor": {
                    "type": "number",
                    "description": "Multiplication factor",
                    "default": 2
                },
                "round": {
                    "type": "boolean",
                    "description": "Round result to nearest integer",
                    "default": true
                }
            }
        })),
    }
}

/// Process function that implements the actor behavior
fn process_actor(context: ActorContext) -> Result<ActorResult, Box<dyn std::error::Error>> {
    let mut outputs = HashMap::new();
    
    // Get configuration values
    let factor = context.config.get_number("factor").unwrap_or(2.0);
    let should_round = context.config.get_bool("round").unwrap_or(true);
    
    // Get the current operation count from state
    let mut operation_count = context.state
        .get("operation_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    
    // Process input
    match context.payload.get("value") {
        Some(Message::Integer(value)) => {
            // Perform multiplication
            let result = *value as f64 * factor;
            let final_result = if should_round {
                result.round() as i64
            } else {
                result as i64
            };
            
            // Update operation count
            operation_count += 1;
            
            // Send result
            outputs.insert("result".to_string(), Message::Integer(final_result));
            
            // Log operation (for demonstration)
            eprintln!("MultiplyActor: {} * {} = {}", value, factor, final_result);
        }
        Some(Message::Float(value)) => {
            // Handle float input
            let result = value * factor;
            let final_result = if should_round {
                result.round() as i64
            } else {
                result as i64
            };
            
            operation_count += 1;
            outputs.insert("result".to_string(), Message::Integer(final_result));
        }
        Some(_) => {
            // Wrong input type
            outputs.insert(
                "error".to_string(), 
                Message::Error("Expected Integer or Float input".to_string())
            );
        }
        None => {
            // No input provided
            outputs.insert(
                "error".to_string(),
                Message::Error("No input provided on 'value' port".to_string())
            );
        }
    }
    
    // Update state with operation count
    let mut new_state = serde_json::Map::new();
    new_state.insert("operation_count".to_string(), operation_count.into());
    new_state.insert("last_factor".to_string(), factor.into());
    
    Ok(ActorResult {
        outputs,
        state: Some(serde_json::Value::Object(new_state)),
    })
}

// Register the plugin using the actor_plugin! macro
actor_plugin!(
    metadata: metadata(),
    process: process_actor
);