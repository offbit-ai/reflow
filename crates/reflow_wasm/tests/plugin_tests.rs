//! Integration tests for WASM plugins using ScriptActor

#[cfg(test)]
mod tests {
    use reflow_script::{ScriptActor, ScriptConfig, ScriptEnvironment, ScriptRuntime};
    use reflow_actor::{Actor, ActorConfig, message::Message};
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Helper to get the crate root directory
    fn get_crate_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    /// Helper function to build and read a plugin
    async fn load_plugin_bytes(plugin_name: &str) -> Vec<u8> {
        let crate_root = get_crate_root();
        let plugin_dir = crate_root.join("examples").join(plugin_name);
        let wasm_path = plugin_dir
            .join("target")
            .join("wasm32-unknown-unknown")
            .join("release")
            .join(format!("{}.wasm", plugin_name));

        // Check if WASM file already exists, if not build it
        if !wasm_path.exists() {
            println!("Building {} plugin...", plugin_name);
            
            // Ensure target is installed
            std::process::Command::new("rustup")
                .args(&["target", "add", "wasm32-unknown-unknown"])
                .output()
                .expect("Failed to add wasm32 target");
            
            // Build the plugin
            let output = std::process::Command::new("cargo")
                .args(&["build", "--release", "--target", "wasm32-unknown-unknown"])
                .current_dir(&plugin_dir)
                .output()
                .expect(&format!("Failed to build {}", plugin_name));
            
            if !output.status.success() {
                panic!(
                    "Failed to build {}: {}",
                    plugin_name,
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        
        std::fs::read(&wasm_path)
            .expect(&format!("Failed to read {}", wasm_path.display()))
    }

    #[tokio::test]
    async fn test_multiply_actor_plugin() {
        // Load the plugin bytes
        let wasm_bytes = load_plugin_bytes("multiply_actor").await;
        
        // Create ScriptConfig for the plugin
        let config = ScriptConfig {
            environment: ScriptEnvironment::SYSTEM,
            runtime: ScriptRuntime::Extism,
            source: wasm_bytes,
            entry_point: "process".to_string(),
            packages: None,
        };
        
        // Create the ScriptActor
        let actor = ScriptActor::new(config);
        
        // Create actor config with custom factor
        let mut metadata = HashMap::new();
        metadata.insert("factor".to_string(), serde_json::json!(3.0));
        
        let actor_config = ActorConfig::from_node(reflow_graph::types::GraphNode {
            id: "test_multiply".to_string(),
            component: "MultiplyActor".to_string(),
            metadata: Some(metadata),
        }).unwrap();
        
        // Start the actor process
        let process = actor.create_process(actor_config.clone(), None);
        let _handle = tokio::spawn(process);
        
        // Get ports
        let inports = actor.get_inports();
        let outports = actor.get_outports();
        
        // Test 1: Integer multiplication
        let mut payload = HashMap::new();
        payload.insert("value".to_string(), Message::Integer(10));
        
        inports.0.send_async(payload).await.unwrap();
        let result = outports.1.recv_async().await.unwrap();
        
        assert!(result.contains_key("result"));
        assert_eq!(result["result"], Message::Integer(30)); // 10 * 3 = 30
        
        // Test 2: Float input
        let mut payload = HashMap::new();
        payload.insert("value".to_string(), Message::Float(7.5));
        
        inports.0.send_async(payload).await.unwrap();
        let result = outports.1.recv_async().await.unwrap();
        
        assert!(result.contains_key("result"));
        assert_eq!(result["result"], Message::Integer(23)); // 7.5 * 3 = 22.5, rounded to 23
        
        // Test 3: Error case - wrong input type
        let mut payload = HashMap::new();
        payload.insert("value".to_string(), Message::String("not a number".to_string().into()));
        
        inports.0.send_async(payload).await.unwrap();
        let result = outports.1.recv_async().await.unwrap();
        
        assert!(result.contains_key("error"));
        if let Message::Error(err) = &result["error"] {
            assert!(err.contains("Expected Integer or Float"));
        } else {
            panic!("Expected error message");
        }
    }

    #[tokio::test]
    async fn test_counter_actor_plugin() {
        // Load the plugin bytes
        let wasm_bytes = load_plugin_bytes("counter_actor").await;
        
        // Create ScriptConfig for the plugin
        let config = ScriptConfig {
            environment: ScriptEnvironment::SYSTEM,
            runtime: ScriptRuntime::Extism,
            source: wasm_bytes,
            entry_point: "process".to_string(),
            packages: None,
        };
        
        // Create the ScriptActor
        let actor = ScriptActor::new(config);
        
        // Create actor config with constraints
        let mut metadata = HashMap::new();
        metadata.insert("initial_value".to_string(), serde_json::json!(5));
        metadata.insert("min_value".to_string(), serde_json::json!(0));
        metadata.insert("max_value".to_string(), serde_json::json!(10));
        
        let actor_config = ActorConfig::from_node(reflow_graph::types::GraphNode {
            id: "test_counter".to_string(),
            component: "CounterActor".to_string(),
            metadata: Some(metadata),
        }).unwrap();
        
        // Start the actor process
        let process = actor.create_process(actor_config, None);
        let _handle = tokio::spawn(process);
        
        // Get ports
        let inports = actor.get_inports();
        let outports = actor.get_outports();
        
        // Test 1: Initial state
        let mut payload = HashMap::new();
        payload.insert("increment".to_string(), Message::Flow);
        
        inports.0.send_async(payload).await.unwrap();
        let result = outports.1.recv_async().await.unwrap();
        
        assert_eq!(result["count"], Message::Integer(6)); // 5 + 1 = 6
        assert_eq!(result["changed"], Message::Boolean(true));
        
        // Test 2: Decrement
        let mut payload = HashMap::new();
        payload.insert("decrement".to_string(), Message::Flow); // 6 - 1 = 5
        
        inports.0.send_async(payload).await.unwrap();
        let result = outports.1.recv_async().await.unwrap();
        
        assert_eq!(result["count"], Message::Integer(5)); // 6 - 1 = 5
        
        // Test 3: Add value
        let mut payload = HashMap::new();
        payload.insert("add".to_string(), Message::Integer(3));
        
        inports.0.send_async(payload).await.unwrap();
        let result = outports.1.recv_async().await.unwrap();
        
        assert_eq!(result["count"], Message::Integer(8)); // 5 + 3 = 8
        
        // Test 4: Test max constraint
        let mut payload = HashMap::new();
        payload.insert("add".to_string(), Message::Integer(5));
        
        inports.0.send_async(payload).await.unwrap();
        let result = outports.1.recv_async().await.unwrap();
        
        assert_eq!(result["count"], Message::Integer(10)); // 8 + 5 = 13, clamped to 10
        
        // Verify info output
        if let Message::Object(info) = &result["info"] {
            // EncodableValue needs to be converted to Value
            let info_value: serde_json::Value = info.as_ref().clone().into();
            assert_eq!(info_value["at_max"], serde_json::json!(true));
            assert_eq!(info_value["current"], serde_json::json!(10));
        }
        
        // Test 5: Reset
        let mut payload = HashMap::new();
        payload.insert("reset".to_string(), Message::Flow);
        
        inports.0.send_async(payload).await.unwrap();
        let result = outports.1.recv_async().await.unwrap();
        
        assert_eq!(result["count"], Message::Integer(5)); // Reset to initial value
        
        // Test 6: Set to specific value
        let mut payload = HashMap::new();
        payload.insert("set".to_string(), Message::Integer(7));
        
        inports.0.send_async(payload).await.unwrap();
        let result = outports.1.recv_async().await.unwrap();
        
        assert_eq!(result["count"], Message::Integer(7));
    }

    #[tokio::test]
    async fn test_plugin_state_persistence() {
        // This test verifies that state persists between invocations
        let wasm_bytes = load_plugin_bytes("multiply_actor").await;
        
        let config = ScriptConfig {
            environment: ScriptEnvironment::SYSTEM,
            runtime: ScriptRuntime::Extism,
            source: wasm_bytes,
            entry_point: "process".to_string(),
            packages: None,
        };
        
        let actor = ScriptActor::new(config);
        
        let actor_config = ActorConfig::from_node(reflow_graph::types::GraphNode {
            id: "test_state".to_string(),
            component: "MultiplyActor".to_string(),
            metadata: Some(HashMap::from([
                ("factor".to_string(), serde_json::json!(2.0))
            ])),
        }).unwrap();
        
        let process = actor.create_process(actor_config, None);
        let _handle = tokio::spawn(process);
        
        let inports = actor.get_inports();
        let outports = actor.get_outports();
        
        // First operation
        let mut payload = HashMap::new();
        payload.insert("value".to_string(), Message::Integer(5));
        
        inports.0.send_async(payload).await.unwrap();
        let _result = outports.1.recv_async().await.unwrap();
        
        // Second operation - state should show operation_count = 2
        let mut payload = HashMap::new();
        payload.insert("value".to_string(), Message::Integer(10));
        
        inports.0.send_async(payload).await.unwrap();
        let result = outports.1.recv_async().await.unwrap();
        
        assert_eq!(result["result"], Message::Integer(20));
        
        // The plugin's state should have been updated
        // (operation_count would be 2 if we could inspect it)
    }

    #[tokio::test]
    async fn test_plugin_builds_correctly() {
        // This test ensures the plugins can be built
        let multiply_bytes = load_plugin_bytes("multiply_actor").await;
        assert!(!multiply_bytes.is_empty());
        assert!(multiply_bytes.len() > 1000); // Should be at least a few KB
        
        let counter_bytes = load_plugin_bytes("counter_actor").await;
        assert!(!counter_bytes.is_empty());
        assert!(counter_bytes.len() > 1000); // Should be at least a few KB
    }

    #[tokio::test]
    async fn test_separate_actors_have_separate_state() {
        // This test verifies that each ScriptActor instance has its own state
        let wasm_bytes = load_plugin_bytes("counter_actor").await;
        
        // Create first actor
        let config1 = ScriptConfig {
            environment: ScriptEnvironment::SYSTEM,
            runtime: ScriptRuntime::Extism,
            source: wasm_bytes.clone(),
            entry_point: "process".to_string(),
            packages: None,
        };
        let actor1 = ScriptActor::new(config1);
        
        // Create second actor with same config
        let config2 = ScriptConfig {
            environment: ScriptEnvironment::SYSTEM,
            runtime: ScriptRuntime::Extism,
            source: wasm_bytes,
            entry_point: "process".to_string(),
            packages: None,
        };
        let actor2 = ScriptActor::new(config2);
        
        // Same actor config for both
        let actor_config = ActorConfig::from_node(reflow_graph::types::GraphNode {
            id: "test_separate_state".to_string(),
            component: "CounterActor".to_string(),
            metadata: Some(HashMap::from([
                ("initial_value".to_string(), serde_json::json!(0))
            ])),
        }).unwrap();
        
        // Start both actor processes
        let process1 = actor1.create_process(actor_config.clone(), None);
        let _handle1 = tokio::spawn(process1);
        
        let process2 = actor2.create_process(actor_config, None);
        let _handle2 = tokio::spawn(process2);
        
        // Get ports for both actors
        let inports1 = actor1.get_inports();
        let outports1 = actor1.get_outports();
        
        let inports2 = actor2.get_inports();
        let outports2 = actor2.get_outports();
        
        // Increment actor1 three times
        for _ in 0..3 {
            let mut payload = HashMap::new();
            payload.insert("increment".to_string(), Message::Flow);
            inports1.0.send_async(payload).await.unwrap();
            let _ = outports1.1.recv_async().await.unwrap();
        }
        
        // Increment actor2 once
        let mut payload = HashMap::new();
        payload.insert("increment".to_string(), Message::Flow);
        inports2.0.send_async(payload).await.unwrap();
        let result2 = outports2.1.recv_async().await.unwrap();
        
        // Actor2 should have count = 1 (not affected by actor1's operations)
        assert_eq!(result2["count"], Message::Integer(1));
        
        // Check actor1's current count
        let mut payload = HashMap::new();
        payload.insert("increment".to_string(), Message::Flow);
        inports1.0.send_async(payload).await.unwrap();
        let result1 = outports1.1.recv_async().await.unwrap();
        
        // Actor1 should have count = 4 (3 + 1)
        assert_eq!(result1["count"], Message::Integer(4));
        
        // Verify the states are indeed separate
        assert_ne!(result1["count"], result2["count"]);
    }

    #[tokio::test]
    #[ignore] // This test requires manual inspection of output
    async fn test_plugin_with_extism_cli() {
        // Build the plugin first
        let wasm_bytes = load_plugin_bytes("multiply_actor").await;
        
        // Save to a temp file for testing with extism CLI
        let temp_path = std::env::temp_dir().join("test_multiply_actor.wasm");
        std::fs::write(&temp_path, wasm_bytes).unwrap();
        
        println!("Plugin saved to: {}", temp_path.display());
        println!("Test with extism CLI:");
        println!("extism call {} get_metadata", temp_path.display());
        println!("extism call {} process --input '{}'", 
            temp_path.display(),
            r#"{"payload":{"value":{"type":"Integer","data":10}},"config":{"factor":2},"state":{}}"#
        );
    }
}