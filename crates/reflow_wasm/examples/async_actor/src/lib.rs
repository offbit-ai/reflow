//! Async actor demonstrating host function usage for asynchronous outputs

use reflow_wasm::*;
use std::collections::HashMap;

/// Plugin metadata
fn metadata() -> PluginMetadata {
    PluginMetadata {
        component: "AsyncActor".to_string(),
        description: "Demonstrates asynchronous output using host functions".to_string(),
        inports: vec![
            port_def!("start", "Start processing", "Flow"),
            port_def!("data", "Data to process", "Array"),
        ],
        outports: vec![
            port_def!("status", "Processing status", "String"),
            port_def!("progress", "Progress updates", "Integer"),
            port_def!("result", "Final result", "Object"),
        ],
        config_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "chunk_size": {
                    "type": "integer",
                    "description": "Size of chunks to process",
                    "default": 10
                },
                "send_progress": {
                    "type": "boolean",
                    "description": "Whether to send progress updates",
                    "default": true
                }
            }
        })),
    }
}

/// Process function that demonstrates async outputs
fn process_actor(context: ActorContext) -> Result<ActorResult, Box<dyn std::error::Error>> {
    let mut outputs = HashMap::new();
    
    // Get configuration
    let chunk_size = context.config.get_integer("chunk_size").unwrap_or(10) as usize;
    let send_progress = context.config.get_bool("send_progress").unwrap_or(true);
    
    // Check if we have data to process
    if let Some(Message::Array(data)) = context.payload.get("data") {
        // Send initial status asynchronously
        let mut status_output = HashMap::new();
        status_output.insert("status".to_string(), Message::String("Starting processing".to_string()));
        host::send_output(status_output)?;
        
        // Process data in chunks
        let total_items = data.len();
        let mut processed_count = 0;
        let mut results = Vec::new();
        
        for (chunk_idx, chunk) in data.chunks(chunk_size).enumerate() {
            // Process chunk
            for item in chunk {
                // Simulate some processing
                if let Some(num) = item.as_i64() {
                    results.push(num * 2);
                }
                processed_count += 1;
            }
            
            // Send progress update asynchronously
            if send_progress {
                let mut progress_output = HashMap::new();
                progress_output.insert("progress".to_string(), 
                    Message::Integer((processed_count * 100 / total_items) as i64));
                progress_output.insert("status".to_string(), 
                    Message::String(format!("Processing chunk {}", chunk_idx + 1)));
                
                host::send_output(progress_output)?;
            }
            
            // Update state with progress
            host::set_state("last_chunk_processed", serde_json::json!(chunk_idx))?;
            host::set_state("items_processed", serde_json::json!(processed_count))?;
        }
        
        // Send completion status
        let mut final_status = HashMap::new();
        final_status.insert("status".to_string(), Message::String("Processing complete".to_string()));
        host::send_output(final_status)?;
        
        // Return final result
        let result_obj = serde_json::json!({
            "processed_count": processed_count,
            "results": results,
            "average": results.iter().sum::<i64>() as f64 / results.len() as f64,
        });
        
        outputs.insert("result".to_string(), Message::Object(result_obj));
    } else if context.payload.contains_key("start") {
        // Just starting - check if we have previous state
        let last_chunk = host::get_state("last_chunk_processed")
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);
        
        let items_processed = host::get_state("items_processed")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        
        // Send status about previous run
        let status_msg = if last_chunk >= 0 {
            format!("Ready to process. Last run: {} items in {} chunks", 
                items_processed, last_chunk + 1)
        } else {
            "Ready to process. No previous runs.".to_string()
        };
        
        outputs.insert("status".to_string(), Message::String(status_msg));
    }
    
    Ok(ActorResult {
        outputs,
        state: None, // State is managed via host functions
    })
}

// Register the plugin
actor_plugin!(
    metadata: metadata(),
    process: process_actor
);