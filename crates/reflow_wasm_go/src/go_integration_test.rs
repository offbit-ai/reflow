#[cfg(test)]
mod go_integration_tests {
    use reflow_script::{ScriptActor, ScriptConfig, ScriptEnvironment, ScriptRuntime};
    use reflow_actor::{Actor, ActorConfig, message::Message};
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::time::timeout;

    fn get_crate_root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[tokio::test]
    async fn test_go_base64_handling() {
        // Build the Go counter plugin
        let crate_root = get_crate_root();
        let example_dir = crate_root.join("examples").join("counter");
        
        // Check if counter.wasm exists, if not try to build it
        let wasm_path = example_dir.join("counter.wasm");
        if !wasm_path.exists() {
            println!("Building counter.wasm...");
            let build_result = std::process::Command::new("tinygo")
                .args(&[
                    "build",
                    "-o", "counter.wasm",
                    "-target", "wasi",
                    "-no-debug",
                    "main.go",
                ])
                .current_dir(&example_dir)
                .status();
                
            if let Err(_) = build_result {
                // Try with the full path to TinyGo
                std::process::Command::new("/Users/amaterasu/tinygo/bin/tinygo")
                    .args(&[
                        "build",
                        "-o", "counter.wasm",
                        "-target", "wasi",
                        "-no-debug",
                        "main.go",
                    ])
                    .current_dir(&example_dir)
                    .status()
                    .expect("Failed to build with TinyGo");
            }
        }
        
        // Load the WASM plugin
        let wasm_bytes = std::fs::read(&wasm_path)
            .expect("Failed to read counter.wasm");
        
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
            id: "test_go_base64".to_string(),
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
        
        // Send a simple request
        let mut payload = HashMap::new();
        payload.insert("operation".to_string(), Message::String("increment".to_string().into()));
        
        inports.0.send_async(payload).await.unwrap();
        
        let result = timeout(Duration::from_secs(5), outports.1.recv_async()).await
            .expect("Timeout waiting for response")
            .expect("Failed to receive response");
        
        println!("Result from Go plugin: {:?}", result);
        
        // Verify the result
        assert!(result.contains_key("value"), "Should have 'value' key");
        assert!(result.contains_key("operation"), "Should have 'operation' key");
        
        if let Message::Integer(val) = &result["value"] {
            assert_eq!(*val, 1, "Value should be 1 after increment");
        }
        
        if let Message::String(op) = &result["operation"] {
            assert_eq!(op.as_str(), "increment", "Operation should be 'increment'");
        }
        
        println!("✅ Go plugin with base64 handling works correctly!");
    }
}