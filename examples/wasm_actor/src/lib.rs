use reflow_wasm::{actor_fn, FnResult, Context, Input, Output, Json};
use std::collections::HashMap;

/// A simple counter actor that demonstrates state management
/// 
/// This actor supports three operations:
/// - increment: Increases the counter by 1
/// - decrement: Decreases the counter by 1
/// - reset: Sets the counter back to 0
#[actor_fn]
pub fn process(input: Input) -> FnResult<Output> {
    // Create context for accessing host functions
    let context = Context::new();
    
    // Get the operation from input
    let operation = input.0.get("operation")
        .and_then(|v| v.as_str())
        .unwrap_or("increment");
    
    // Get current counter value from state
    let counter_key = "counter";
    let current_counter = match context.get_state(counter_key) {
        Ok(value) => value.as_i64().unwrap_or(0),
        Err(_) => 0, // Default to 0 if not found
    };
    
    // Process based on operation
    let new_counter = match operation {
        "increment" => current_counter + 1,
        "decrement" => current_counter - 1,
        "reset" => 0,
        _ => current_counter, // Default: no change
    };
    
    // Store the new counter value
    context.set_state(counter_key, serde_json::json!(new_counter))?;
    
    // Prepare the output
    let mut output = HashMap::new();
    output.insert("value".to_string(), serde_json::json!(new_counter));
    output.insert("previous".to_string(), serde_json::json!(current_counter));
    output.insert("operation".to_string(), serde_json::json!(operation));
    
    // Send the output to any connected ports
    context.send_output(output.clone())?;
    
    // Return the result
    Ok(Json(output))
}
