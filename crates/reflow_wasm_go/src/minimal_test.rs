#[cfg(test)]
mod minimal_tests {
    use reflow_script::{ScriptActor, ScriptConfig, ScriptEnvironment, ScriptRuntime};
    use reflow_actor::{Actor, ActorConfig, message::Message};
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::time::timeout;

    fn get_crate_root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[tokio::test]
    async fn test_minimal_go_counter() {
        // Load the Go counter plugin
        let crate_root = get_crate_root();
        let wasm_path = crate_root
            .join("examples")
            .join("counter")
            .join("counter.wasm");
        
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
            id: "test_minimal_go_counter".to_string(),
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
        
        // Send empty payload (no operation)
        let payload = HashMap::new();
        
        println!("Sending empty payload");
        inports.0.send_async(payload).await.unwrap();
        
        let result = timeout(Duration::from_secs(5), outports.1.recv_async()).await
            .expect("Timeout waiting for response")
            .expect("Failed to receive response");
        
        println!("Minimal Go counter result: {:?}", result);
        
        // Check if we get any result at all
        assert!(!result.is_empty(), "Result should not be empty");
        
        // If successful, check for value
        if result.contains_key("value") {
            println!("SUCCESS: Got value in result");
            if let Message::Integer(val) = &result["value"] {
                println!("Value is integer: {}", val);
            }
        } else {
            println!("No 'value' key found, available keys: {:?}", result.keys().collect::<Vec<_>>());
        }
    }
}