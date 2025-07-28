//! Stateful counter actor plugin demonstrating state management

use reflow_wasm::*;
use std::collections::HashMap;

/// Plugin metadata
fn metadata() -> PluginMetadata {
    PluginMetadata {
        component: "CounterActor".to_string(),
        description: "A stateful counter that can increment, decrement, and reset".to_string(),
        inports: vec![
            port_def!("increment", "Increment the counter", "Flow"),
            port_def!("decrement", "Decrement the counter", "Flow"),
            port_def!("add", "Add a specific value", "Integer"),
            port_def!("reset", "Reset counter to zero", "Flow"),
            port_def!("set", "Set counter to specific value", "Integer"),
        ],
        outports: vec![
            port_def!("count", "Current counter value", "Integer"),
            port_def!("changed", "Emitted when count changes", "Boolean"),
            port_def!("info", "Counter information", "Object"),
        ],
        config_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "initial_value": {
                    "type": "integer",
                    "description": "Initial counter value",
                    "default": 0
                },
                "min_value": {
                    "type": "integer",
                    "description": "Minimum allowed value",
                    "default": null
                },
                "max_value": {
                    "type": "integer",
                    "description": "Maximum allowed value",
                    "default": null
                }
            }
        })),
    }
}

/// Process function
fn process_actor(context: ActorContext) -> Result<ActorResult, Box<dyn std::error::Error>> {
    let mut outputs = HashMap::new();
    
    // Get configuration
    let initial_value = context.config.get_integer("initial_value").unwrap_or(0);
    let min_value = context.config.get_integer("min_value");
    let max_value = context.config.get_integer("max_value");
    
    // Get current state
    let mut count = context.state
        .get("count")
        .and_then(|v| v.as_i64())
        .unwrap_or(initial_value);
    
    let mut total_operations = context.state
        .get("total_operations")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    
    let old_count = count;
    let mut changed = false;
    
    // Process inputs
    if context.payload.contains_key("increment") {
        count += 1;
        changed = true;
        total_operations += 1;
    }
    
    if context.payload.contains_key("decrement") {
        count -= 1;
        changed = true;
        total_operations += 1;
    }
    
    if let Some(Message::Integer(value)) = context.payload.get("add") {
        count += value;
        changed = true;
        total_operations += 1;
    }
    
    if context.payload.contains_key("reset") {
        count = initial_value;
        changed = true;
        total_operations += 1;
    }
    
    if let Some(Message::Integer(value)) = context.payload.get("set") {
        count = *value;
        changed = true;
        total_operations += 1;
    }
    
    // Apply constraints
    if let Some(min) = min_value {
        count = count.max(min);
    }
    if let Some(max) = max_value {
        count = count.min(max);
    }
    
    // Always output current count
    outputs.insert("count".to_string(), Message::Integer(count));
    
    // Output changed signal if count changed
    if changed {
        outputs.insert("changed".to_string(), Message::Boolean(true));
    }
    
    // Output info object
    let info = serde_json::json!({
        "current": count,
        "previous": old_count,
        "total_operations": total_operations,
        "min_value": min_value,
        "max_value": max_value,
        "at_min": min_value.map(|min| count == min).unwrap_or(false),
        "at_max": max_value.map(|max| count == max).unwrap_or(false),
    });
    outputs.insert("info".to_string(), Message::Object(info));
    
    // Update state
    let mut new_state = serde_json::Map::new();
    new_state.insert("count".to_string(), count.into());
    new_state.insert("total_operations".to_string(), total_operations.into());
    // Note: timestamp would require a host function or other approach in WASM
    new_state.insert("last_operation".to_string(), 
        serde_json::Value::String(format!("operation_{}", total_operations))
    );
    
    Ok(ActorResult {
        outputs,
        state: Some(serde_json::Value::Object(new_state)),
    })
}

// Register the plugin
actor_plugin!(
    metadata: metadata(),
    process: process_actor
);