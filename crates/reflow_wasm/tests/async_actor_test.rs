//! Test for async actor demonstrating host function usage

#[cfg(test)]
mod tests {
    use reflow_script::{ScriptActor, ScriptConfig, ScriptEnvironment, ScriptRuntime};
    use reflow_actor::{Actor, ActorConfig, message::Message};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::Duration;
    use tokio::time::timeout;

    fn get_crate_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[tokio::test]
    async fn test_async_actor_sends_multiple_outputs() {
        // Load the async actor plugin
        let crate_root = get_crate_root();
        let wasm_path = crate_root
            .join("examples")
            .join("async_actor")
            .join("target")
            .join("wasm32-unknown-unknown")
            .join("release")
            .join("async_actor.wasm");
        
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
        metadata.insert("chunk_size".to_string(), serde_json::json!(5));
        metadata.insert("send_progress".to_string(), serde_json::json!(true));
        
        let actor_config = ActorConfig::from_node(reflow_graph::types::GraphNode {
            id: "test_async".to_string(),
            component: "AsyncActor".to_string(),
            metadata: Some(metadata),
            ..Default::default()
        }).unwrap();
        
        // Start the actor process
        let process = actor.create_process(actor_config, None);
        let _handle = tokio::spawn(process);
        
        // Get ports
        let inports = actor.get_inports();
        let outports = actor.get_outports();
        
        // Test 1: Send start signal to check state
        let mut payload = HashMap::new();
        payload.insert("start".to_string(), Message::Flow);
        
        inports.0.send_async(payload).await.unwrap();
        let result = timeout(Duration::from_secs(2), outports.1.recv_async()).await
            .expect("Timeout waiting for response")
            .expect("Failed to receive response");
        
        println!("Debug: Received result: {:?}", result);
        
        // Should get status about no previous runs
        assert!(result.contains_key("status"), "Result keys: {:?}", result.keys().collect::<Vec<_>>());
        if let Message::String(status) = &result["status"] {
            assert!(status.contains("No previous runs"));
        }
        
        // Test 2: Send data to process - this should trigger multiple async outputs
        let test_data: Vec<reflow_actor::message::EncodableValue> = (0..20)
            .map(|i| reflow_actor::message::EncodableValue::from(serde_json::json!(i)))
            .collect();
        
        let mut payload = HashMap::new();
        payload.insert("data".to_string(), Message::Array(test_data.into()));
        
        inports.0.send_async(payload).await.unwrap();
        
        // Collect all outputs until we get the final result
        let mut outputs = Vec::new();
        let mut got_final_result = false;
        
        // Set a timeout for the entire collection
        let collection_timeout = timeout(Duration::from_secs(5), async {
            while !got_final_result {
                if let Ok(output) = outports.1.recv_async().await {
                    println!("Received output: {:?}", output);
                    
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
        assert!(outputs.len() >= 3, "Expected at least 3 outputs (initial status, progress, final result), got {}", outputs.len());
        
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
        
        // Check for completion status
        let has_completion_status = outputs.iter().any(|o| {
            o.get("status").map(|s| {
                if let Message::String(str) = s {
                    str.contains("Processing complete")
                } else {
                    false
                }
            }).unwrap_or(false)
        });
        assert!(has_completion_status, "Should have completion status");
        
        // Check final result
        let final_result = outputs.iter().find(|o| o.contains_key("result"));
        assert!(final_result.is_some(), "Should have final result");
        
        if let Some(result_output) = final_result {
            if let Message::Object(obj) = &result_output["result"] {
                let obj_val: serde_json::Value = obj.as_ref().clone().into();
                assert_eq!(obj_val["processed_count"], serde_json::json!(20));
                
                // Check results array (each number doubled)
                if let Some(results) = obj_val["results"].as_array() {
                    assert_eq!(results.len(), 20);
                    assert_eq!(results[0], serde_json::json!(0));
                    assert_eq!(results[19], serde_json::json!(38)); // 19 * 2
                }
                
                // Check average
                assert_eq!(obj_val["average"], serde_json::json!(19.0)); // Average of 0,2,4...38
            }
        }
        
        // Test 3: Send another start to check state persistence
        let mut payload = HashMap::new();
        payload.insert("start".to_string(), Message::Flow);
        
        inports.0.send_async(payload).await.unwrap();
        let result = timeout(Duration::from_secs(2), outports.1.recv_async()).await
            .expect("Timeout waiting for response")
            .expect("Failed to receive response");
        
        // Should show previous run info
        assert!(result.contains_key("status"));
        if let Message::String(status) = &result["status"] {
            assert!(status.contains("Last run: 20 items"));
        }
    }
    
    #[tokio::test]
    async fn test_async_actor_without_progress() {
        // Load the async actor plugin
        let crate_root = get_crate_root();
        let wasm_path = crate_root
            .join("examples")
            .join("async_actor")
            .join("target")
            .join("wasm32-unknown-unknown")
            .join("release")
            .join("async_actor.wasm");
        
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
        
        // Create actor config with progress disabled
        let mut metadata = HashMap::new();
        metadata.insert("chunk_size".to_string(), serde_json::json!(10));
        metadata.insert("send_progress".to_string(), serde_json::json!(false));
        
        let actor_config = ActorConfig::from_node(reflow_graph::types::GraphNode {
            id: "test_async_no_progress".to_string(),
            component: "AsyncActor".to_string(),
            metadata: Some(metadata),
            ..Default::default()
        }).unwrap();
        
        // Start the actor process
        let process = actor.create_process(actor_config, None);
        let _handle = tokio::spawn(process);
        
        // Get ports
        let inports = actor.get_inports();
        let outports = actor.get_outports();
        
        // Send data to process
        let test_data: Vec<reflow_actor::message::EncodableValue> = (0..10)
            .map(|i| reflow_actor::message::EncodableValue::from(serde_json::json!(i)))
            .collect();
        
        let mut payload = HashMap::new();
        payload.insert("data".to_string(), Message::Array(test_data.into()));
        
        inports.0.send_async(payload).await.unwrap();
        
        // Collect outputs
        let mut outputs = Vec::new();
        let mut got_final_result = false;
        
        let collection_timeout = timeout(Duration::from_secs(3), async {
            while !got_final_result {
                if let Ok(output) = outports.1.recv_async().await {
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
        
        // Should have fewer outputs when progress is disabled
        assert!(outputs.len() >= 2, "Should have at least initial status and result");
        
        // Should NOT have progress updates
        let progress_updates: Vec<_> = outputs.iter().filter(|o| o.contains_key("progress")).collect();
        assert!(progress_updates.is_empty(), "Should not have progress updates when disabled");
    }
}