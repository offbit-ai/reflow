#[cfg(test)]
mod basic_tests {
    use reflow_script::{ScriptActor, ScriptConfig, ScriptEnvironment, ScriptRuntime};
    use reflow_actor::{Actor, ActorConfig, message::Message};
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::time::timeout;

    fn get_crate_root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[tokio::test]
    async fn test_basic_go_counter() {
        // Build the WASM plugin first
        let crate_root = get_crate_root();
        let example_dir = crate_root.join("examples").join("counter");
        
        // Try to build with TinyGo
        let build_result = std::process::Command::new("/Users/amaterasu/tinygo/bin/tinygo")
            .args(&[
                "build",
                "-o", "counter.wasm",
                "-target", "wasi",
                "-no-debug",
                "main.go",
            ])
            .current_dir(&example_dir)
            .status();
            
        match build_result {
            Ok(status) if status.success() => {
                println!("✓ Successfully built counter.wasm with TinyGo");
            }
            _ => {
                // Try with standard Go as fallback
                println!("TinyGo failed, trying standard Go...");
                let go_result = std::process::Command::new("go")
                    .env("GOOS", "wasip1")
                    .env("GOARCH", "wasm")
                    .args(&[
                        "build",
                        "-o", "counter.wasm",
                        "main.go",
                    ])
                    .current_dir(&example_dir)
                    .status()
                    .expect("Failed to run Go compiler");
                    
                if !go_result.success() {
                    panic!("Failed to build WASM with both TinyGo and standard Go");
                }
                println!("✓ Successfully built counter.wasm with standard Go");
            }
        }
        
        // Load the Go counter plugin
        let wasm_path = example_dir.join("counter.wasm");
        println!("Loading WASM from: {}", wasm_path.display());
        
        let wasm_bytes = match std::fs::read(&wasm_path) {
            Ok(bytes) => {
                println!("Successfully read {} bytes", bytes.len());
                bytes
            }
            Err(e) => {
                panic!("Failed to read WASM file: {}", e);
            }
        };
        
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
            id: "test_basic_go_counter".to_string(),
            component: "GoCounter".to_string(),
            metadata: None,
        }).unwrap();
        
        // Start the actor process
        let process = actor.create_process(actor_config, None);
        let _handle = tokio::spawn(process);
        
        // Get ports
        let inports = actor.get_inports();
        let outports = actor.get_outports();
        
        // Test 1: Send empty payload (no operation) - should return count 0
        println!("\n=== Test 1: Initial state ===");
        let payload = HashMap::new();
        
        inports.0.send_async(payload).await.unwrap();
        
        let result = timeout(Duration::from_secs(5), outports.1.recv_async()).await
            .expect("Timeout waiting for response")
            .expect("Failed to receive response");
        
        println!("Initial result: {:?}", result);
        
        assert!(result.contains_key("value"), "Should have 'value' key");
        if let Message::Integer(val) = &result["value"] {
            assert_eq!(*val, 0, "Initial count should be 0");
            println!("✓ Initial count is 0");
        } else {
            panic!("Expected Integer message for value");
        }
        
        // Test 2: Send increment operation
        println!("\n=== Test 2: Increment operation ===");
        let mut payload = HashMap::new();
        payload.insert("operation".to_string(), Message::String("increment".to_string().into()));
        
        inports.0.send_async(payload).await.unwrap();
        
        let result = timeout(Duration::from_secs(5), outports.1.recv_async()).await
            .expect("Timeout waiting for response")
            .expect("Failed to receive response");
        
        println!("Increment result: {:?}", result);
        
        assert!(result.contains_key("value"), "Should have 'value' key");
        assert!(result.contains_key("operation"), "Should have 'operation' key");
        
        if let Message::Integer(val) = &result["value"] {
            assert_eq!(*val, 1, "Count after increment should be 1");
            println!("✓ Count after increment is 1");
        } else {
            panic!("Expected Integer message for value");
        }
        
        if let Message::String(op) = &result["operation"] {
            assert_eq!(op.as_str(), "increment", "Operation should be 'increment'");
            println!("✓ Operation is 'increment'");
        } else {
            panic!("Expected String message for operation");
        }
        
        println!("\n✅ All basic tests passed!");
    }
}