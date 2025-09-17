#[cfg(test)]
mod host_function_tests {
    use reflow_script::{ScriptActor, ScriptConfig, ScriptEnvironment, ScriptRuntime};
    use reflow_actor::{Actor, ActorConfig, message::Message};
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::time::timeout;

    fn get_crate_root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[tokio::test]
    async fn test_go_counter_with_host_functions() {
        // First, build the WASM plugin
        let crate_root = get_crate_root();
        let example_dir = crate_root.join("examples").join("counter_with_state");
        
        // Build using TinyGo
        let build_result = std::process::Command::new("/Users/amaterasu/tinygo/bin/tinygo")
            .args(&[
                "build",
                "-o", "counter_with_state.wasm",
                "-target", "wasi",
                "-no-debug",
                "main.go",
            ])
            .current_dir(&example_dir)
            .status();
            
        match build_result {
            Ok(status) if status.success() => {
                println!("✓ Successfully built counter_with_state.wasm");
            }
            Ok(status) => {
                panic!("TinyGo build failed with status: {}", status);
            }
            Err(e) => {
                panic!("Failed to run TinyGo: {}", e);
            }
        }
        
        // Load the WASM plugin
        let wasm_path = example_dir.join("counter_with_state.wasm");
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
            id: "test_go_counter_with_state".to_string(),
            component: "GoCounterWithState".to_string(),
            metadata: None,
            ..Default::default()
        }).unwrap();
        
        // Start the actor process
        let process = actor.create_process(actor_config, None);
        let _handle = tokio::spawn(process);
        
        // Get ports
        let inports = actor.get_inports();
        let outports = actor.get_outports();
        
        // Test 1: Initial state (should be 0)
        println!("\n=== Test 1: Initial State ===");
        let mut payload = HashMap::new();
        inports.0.send_async(payload).await.unwrap();
        
        let result = timeout(Duration::from_secs(5), outports.1.recv_async()).await
            .expect("Timeout waiting for response")
            .expect("Failed to receive response");
        
        println!("Initial state result: {:?}", result);
        if let Some(Message::Integer(value)) = result.get("value") {
            assert_eq!(*value, 0, "Initial count should be 0");
            println!("✓ Initial count is correct: {}", value);
        } else {
            panic!("Expected integer value in result");
        }
        
        // Test 2: Increment operation
        println!("\n=== Test 2: Increment Operation ===");
        let mut payload = HashMap::new();
        payload.insert("operation".to_string(), Message::String("increment".to_string().into()));
        inports.0.send_async(payload).await.unwrap();
        
        let result = timeout(Duration::from_secs(5), outports.1.recv_async()).await
            .expect("Timeout waiting for response")
            .expect("Failed to receive response");
        
        println!("Increment result: {:?}", result);
        // The async output sends status and count
        if let Some(Message::Integer(count)) = result.get("count") {
            assert_eq!(*count, 1, "Count after increment should be 1");
            println!("✓ Count after increment is correct: {}", count);
        } else if let Some(Message::Integer(value)) = result.get("value") {
            assert_eq!(*value, 1, "Count after increment should be 1");
            println!("✓ Count after increment is correct: {}", value);
        } else {
            panic!("Expected integer value in result");
        }
        
        // Test 3: Double operation (should use state from previous)
        println!("\n=== Test 3: Double Operation ===");
        
        // Drain any pending async messages
        while let Ok(msg) = timeout(Duration::from_millis(100), outports.1.recv_async()).await {
            println!("Draining pending message: {:?}", msg);
        }
        
        let mut payload = HashMap::new();
        payload.insert("operation".to_string(), Message::String("double".to_string().into()));
        inports.0.send_async(payload).await.unwrap();
        
        let mut result = timeout(Duration::from_secs(5), outports.1.recv_async()).await
            .expect("Timeout waiting for response")
            .expect("Failed to receive response");
        
        println!("Double result: {:?}", result);
        // Check if we got an async output or the actual result
        if let Some(Message::String(_status)) = result.get("status") {
            // This is an async output, wait for the actual result
            println!("Got async output, waiting for actual result...");
            result = timeout(Duration::from_secs(5), outports.1.recv_async()).await
                .expect("Timeout waiting for response")
                .expect("Failed to receive response");
            println!("Actual double result: {:?}", result);
        }
        
        if let Some(Message::Integer(value)) = result.get("value") {
            assert_eq!(*value, 2, "Count after double should be 2 (1 * 2)");
            println!("✓ Count after double is correct: {}", value);
        } else if let Some(Message::Integer(count)) = result.get("count") {
            assert_eq!(*count, 2, "Count after double should be 2 (1 * 2)");
            println!("✓ Count after double is correct: {}", count);
        } else {
            panic!("Expected integer value in result");
        }
        
        // Test 4: Reset operation
        println!("\n=== Test 4: Reset Operation ===");
        let mut payload = HashMap::new();
        payload.insert("operation".to_string(), Message::String("reset".to_string().into()));
        inports.0.send_async(payload).await.unwrap();
        
        let mut result = timeout(Duration::from_secs(5), outports.1.recv_async()).await
            .expect("Timeout waiting for response")
            .expect("Failed to receive response");
        
        println!("Reset result: {:?}", result);
        // Check if we got an async output or the actual result
        if let Some(Message::String(_status)) = result.get("status") {
            // This is an async output, wait for the actual result
            println!("Got async output, waiting for actual result...");
            result = timeout(Duration::from_secs(5), outports.1.recv_async()).await
                .expect("Timeout waiting for response")
                .expect("Failed to receive response");
            println!("Actual reset result: {:?}", result);
        }
        
        if let Some(Message::Integer(value)) = result.get("value") {
            assert_eq!(*value, 0, "Count after reset should be 0");
            println!("✓ Count after reset is correct: {}", value);
        } else if let Some(Message::Integer(count)) = result.get("count") {
            assert_eq!(*count, 0, "Count after reset should be 0");
            println!("✓ Count after reset is correct: {}", count);
        } else {
            panic!("Expected integer value in result");
        }
        
        println!("\n✅ All host function tests passed!");
    }
}