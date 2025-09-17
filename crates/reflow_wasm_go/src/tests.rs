//! Tests for Go WASM plugins

use crate::utils::*;
use reflow_script::{ScriptActor, ScriptConfig, ScriptEnvironment, ScriptRuntime};
use reflow_actor::{Actor, ActorConfig, message::Message};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

fn get_crate_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[tokio::test]
async fn test_go_counter_actor() {
    // Load the Go counter plugin
    let crate_root = get_crate_root();
    let wasm_path = crate_root
        .join("examples")
        .join("counter")
        .join("counter.wasm");
    
    let wasm_bytes = std::fs::read(&wasm_path)
        .expect(&format!("Failed to read {}", wasm_path.display()));
    
    // Create ScriptConfig
    let config = ScriptConfig {
        environment: ScriptEnvironment::SYSTEM,
        runtime: ScriptRuntime::Extism,
        source: wasm_bytes,
        entry_point: "process".to_string(),
        packages: None,
    };
    
    // Create the ScriptActor
    let actor = ScriptActor::new(config);
    
    // Create actor config
    let actor_config = ActorConfig::from_node(reflow_graph::types::GraphNode {
        id: "test_go_counter".to_string(),
        component: "GoCounter".to_string(),
        metadata: None,
        ..Default::default()
    }).unwrap();
    
    // Start the actor process
    let process = actor.create_process(actor_config, None);
    let _handle = tokio::spawn(process);
    
    // Get ports
    let inports = actor.get_inports();
    let outports = actor.get_outports();
    
    // Test increment operation
    let mut payload = HashMap::new();
    payload.insert("operation".to_string(), Message::string("increment".to_string()));
    
    inports.0.send_async(payload).await.unwrap();
    let result = timeout(Duration::from_secs(2), outports.1.recv_async()).await
        .expect("Timeout waiting for response")
        .expect("Failed to receive response");
    
    println!("Go counter result: {:?}", result);
    
    // Verify the result
    assert!(result.contains_key("value"), "Result should contain 'value' key");
    assert_eq!(result["value"], Message::Integer(1));
    assert!(result.contains_key("operation"), "Result should contain 'operation' key");
    assert_eq!(result["operation"], Message::String("increment".to_string().into()));
}

#[tokio::test]
async fn test_go_async_processor() {
    // Load the Go async processor plugin
    let crate_root = get_crate_root();
    let wasm_path = crate_root
        .join("examples")
        .join("async_processor")
        .join("async_processor.wasm");
    
    let wasm_bytes = std::fs::read(&wasm_path)
        .expect(&format!("Failed to read {}", wasm_path.display()));
    
    // Create ScriptConfig
    let config = ScriptConfig {
        environment: ScriptEnvironment::SYSTEM,
        runtime: ScriptRuntime::Extism,
        source: wasm_bytes,
        entry_point: "process".to_string(),
        packages: None,
    };
    
    // Create the ScriptActor
    let actor = ScriptActor::new(config);
    
    // Create actor config
    let mut metadata = HashMap::new();
    metadata.insert("chunk_size".to_string(), serde_json::json!(3));
    metadata.insert("send_progress".to_string(), serde_json::json!(true));
    
    let actor_config = ActorConfig::from_node(reflow_graph::types::GraphNode {
        id: "test_go_async".to_string(),
        component: "GoAsyncProcessor".to_string(),
        metadata: Some(metadata),
        ..Default::default()
    }).unwrap();
    
    // Start the actor process
    let process = actor.create_process(actor_config, None);
    let _handle = tokio::spawn(process);
    
    // Get ports
    let inports = actor.get_inports();
    let outports = actor.get_outports();
    
    // Test with start message first
    let mut payload = HashMap::new();
    payload.insert("start".to_string(), Message::Flow);
    
    inports.0.send_async(payload).await.unwrap();
    let result = timeout(Duration::from_secs(2), outports.1.recv_async()).await
        .expect("Timeout waiting for response")
        .expect("Failed to receive response");
    
    println!("Go async start result: {:?}", result);
    
    // Should get status about no previous runs
    assert!(result.contains_key("status"));
    if let Message::String(status) = &result["status"] {
        assert!(status.contains("No previous runs"));
    }
    
    // Test with data processing
    let test_data: Vec<reflow_actor::message::EncodableValue> = (0..9)
        .map(|i| reflow_actor::message::EncodableValue::from(serde_json::json!(i)))
        .collect();
    
    let mut payload = HashMap::new();
    payload.insert("data".to_string(), Message::Array(test_data.into()));
    
    inports.0.send_async(payload).await.unwrap();
    
    // Collect all outputs until we get the final result
    let mut outputs = Vec::new();
    let mut got_final_result = false;
    
    let collection_timeout = timeout(Duration::from_secs(5), async {
        while !got_final_result {
            if let Ok(output) = outports.1.recv_async().await {
                println!("Go async output: {:?}", output);
                
                // Check if this is the final result
                if output.contains_key("result") {
                    got_final_result = true;
                }
                
                outputs.push(output);
            } else {
                break;
            }
        }
    }).await;
    
    assert!(collection_timeout.is_ok(), "Timeout collecting outputs");
    
    // Verify we got multiple outputs
    assert!(outputs.len() >= 3, 
        "Expected at least 3 outputs (initial status, progress, final result), got {}", 
        outputs.len());
    
    // Check for initial status
    let has_initial_status = outputs.iter().any(|o| {
        o.get("status").map(|s| {
            if let Message::String(str) = s {
                str.contains("Starting processing")
            } else {
                false
            }
        }).unwrap_or(false)
    });
    assert!(has_initial_status, "Should have initial status");
    
    // Check for progress updates
    let progress_updates: Vec<_> = outputs.iter().filter(|o| o.contains_key("progress")).collect();
    assert!(!progress_updates.is_empty(), "Should have progress updates");
    
    // Check final result
    let final_result = outputs.iter().find(|o| o.contains_key("result"));
    assert!(final_result.is_some(), "Should have final result");
    
    if let Some(result_output) = final_result {
        if let Message::Object(obj) = &result_output["result"] {
            let obj_val: serde_json::Value = obj.as_ref().clone().into();
            assert_eq!(obj_val["processed_count"], serde_json::json!(9));
            
            // Check results array (each number doubled: 0, 2, 4, 6, 8, 10, 12, 14, 16)
            if let Some(results) = obj_val["results"].as_array() {
                assert_eq!(results.len(), 9);
                assert_eq!(results[0], serde_json::json!(0)); // 0*2
                assert_eq!(results[3], serde_json::json!(6)); // 3*2
                assert_eq!(results[8], serde_json::json!(16)); // 8*2
            }
        }
    }
}