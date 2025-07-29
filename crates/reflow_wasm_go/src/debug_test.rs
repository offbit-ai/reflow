#[cfg(test)]
mod debug_tests {
    use extism::{Manifest, Plugin, Wasm, WasmMetadata};
    use std::collections::HashMap;
    
    #[test]
    fn test_debug_wasm() {
        // Load the debug WASM file
        let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join("debug")
            .join("debug.wasm");
            
        if !wasm_path.exists() {
            panic!("debug.wasm not found at {:?}", wasm_path);
        }
        
        let wasm_bytes = std::fs::read(&wasm_path).expect("Failed to read debug.wasm");
        
        // Create a manifest
        let wasm = Wasm::Data {
            data: wasm_bytes,
            meta: WasmMetadata::default(),
        };
        let manifest = Manifest::default().with_wasm(wasm);
        
        // Create plugin
        let mut plugin = Plugin::new(&manifest, [], true).expect("Failed to create plugin");
        
        // Test get_metadata
        println!("Testing get_metadata...");
        let metadata_result = plugin.call::<(), String>("get_metadata", ());
        match metadata_result {
            Ok(metadata) => println!("Metadata: {}", metadata),
            Err(e) => println!("get_metadata error: {:?}", e),
        }
        
        // Test process
        println!("\nTesting process...");
        let input = serde_json::json!({
            "payload": {},
            "config": {
                "node_id": "test",
                "component": "test",
                "resolved_env": {},
                "config": {},
                "namespace": null
            },
            "state": {}
        });
        
        let process_result = plugin.call::<serde_json::Value, String>("process", input);
        match process_result {
            Ok(result) => println!("Process result: {}", result),
            Err(e) => println!("process error: {:?}", e),
        }
    }
}