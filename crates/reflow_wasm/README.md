# Reflow WebAssembly Plugin SDK

This SDK enables developers to build Reflow actors as WebAssembly plugins using the Extism PDK. Plugins can be written in any language that compiles to WebAssembly, including Rust, Go, Zig, and more.

## Quick Start

### Creating a Plugin in Rust

1. Create a new Rust project:
```bash
cargo new --lib my_reflow_plugin
cd my_reflow_plugin
```

2. Add dependencies to `Cargo.toml`:
```toml
[package]
name = "my_reflow_plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
reflow_wasm = "0.1"
anyhow = "1.0"
serde_json = "1.0"

[profile.release]
opt-level = "z"
lto = true
```

3. Implement your actor in `src/lib.rs`:
```rust
use reflow_wasm::*;
use anyhow::Result;
use std::collections::HashMap;

// Define plugin metadata
fn metadata() -> PluginMetadata {
    PluginMetadata {
        component: "MultiplyActor".to_string(),
        description: "Multiplies input by a configurable factor".to_string(),
        inports: vec![
            port_def!("input", "Number to multiply", "Integer", required),
        ],
        outports: vec![
            port_def!("output", "Multiplied result", "Integer"),
        ],
        config_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "factor": {
                    "type": "number",
                    "description": "Multiplication factor",
                    "default": 2
                }
            }
        })),
    }
}

// Implement actor behavior
fn process_actor(context: ActorContext) -> Result<ActorResult> {
    let mut outputs = HashMap::new();
    
    // Get configuration
    let factor = context.config.get_number("factor").unwrap_or(2.0);
    
    // Get input value
    if let Some(Message::Integer(value)) = context.payload.get("input") {
        // Process the value
        let result = (*value as f64 * factor) as i64;
        
        // Send to output
        outputs.insert("output".to_string(), Message::Integer(result));
    }
    
    Ok(ActorResult {
        outputs,
        state: None, // No state changes
    })
}

// Register the plugin
actor_plugin!(
    metadata: metadata(),
    process: process_actor
);
```

4. Build the plugin:
```bash
cargo build --release --target wasm32-unknown-unknown
```

The compiled WASM file will be at `target/wasm32-unknown-unknown/release/my_reflow_plugin.wasm`.

## Plugin Development Guide

### Message Types

The SDK provides message types that match Reflow's actor system:

```rust
pub enum Message {
    Flow,                       // Flow control signal
    Event(Value),              // Event with JSON data
    Boolean(bool),             // Boolean value
    Integer(i64),              // 64-bit integer
    Float(f64),                // 64-bit float
    String(String),            // UTF-8 string
    Object(Value),             // JSON object
    Array(Vec<Value>),         // Array of JSON values
    Stream(Vec<u8>),           // Binary data stream
    Optional(Option<Box<Value>>), // Optional value
    Any(Value),                // Any JSON value
    Error(String),             // Error message
}
```

### Actor Context

Each plugin receives an `ActorContext` containing:

```rust
pub struct ActorContext {
    /// Input messages keyed by port name
    pub payload: HashMap<String, Message>,
    
    /// Actor configuration
    pub config: ActorConfig,
    
    /// Current state (as JSON)
    pub state: serde_json::Value,
}
```

### Configuration Access

The `ActorConfig` provides helper methods for accessing configuration:

```rust
// Get string value
let api_key = context.config.get_string("api_key");

// Get number value
let timeout = context.config.get_number("timeout");

// Get boolean value
let debug = context.config.get_bool("debug");

// Get integer value
let retries = context.config.get_integer("max_retries");

// Get environment variable
let db_url = context.config.get_env("DATABASE_URL");
```

### State Management

Plugins can maintain state between invocations:

```rust
fn process_actor(mut context: ActorContext) -> Result<ActorResult> {
    // Get current state
    let mut counter = context.state
        .get("counter")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    
    // Update state
    counter += 1;
    
    // Return updated state
    let mut new_state = serde_json::Map::new();
    new_state.insert("counter".to_string(), counter.into());
    
    Ok(ActorResult {
        outputs: HashMap::new(),
        state: Some(serde_json::Value::Object(new_state)),
    })
}
```

### Helper Functions

The SDK provides helper functions for common operations:

```rust
use reflow_wasm::helpers::*;

// Get typed input
let value: i64 = get_input(&context, "input").unwrap_or(0);

// Create output
let output = create_output("result", Message::Integer(42));

// Error result
if something_wrong {
    return Ok(error_result("Something went wrong"));
}

// Success result with single output
return Ok(success_result("output", Message::String("Done".to_string())));
```

## Advanced Examples

### Stateful Counter Actor

```rust
use reflow_wasm::*;
use anyhow::Result;
use std::collections::HashMap;

fn metadata() -> PluginMetadata {
    PluginMetadata {
        component: "Counter".to_string(),
        description: "Stateful counter actor".to_string(),
        inports: vec![
            port_def!("increment", "Increment signal", "Flow"),
            port_def!("reset", "Reset signal", "Flow"),
        ],
        outports: vec![
            port_def!("count", "Current count", "Integer"),
        ],
        config_schema: None,
    }
}

fn process_actor(context: ActorContext) -> Result<ActorResult> {
    let mut outputs = HashMap::new();
    
    // Get current count from state
    let mut count = context.state
        .get("count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    
    // Handle inputs
    if context.payload.contains_key("increment") {
        count += 1;
    }
    
    if context.payload.contains_key("reset") {
        count = 0;
    }
    
    // Send current count
    outputs.insert("count".to_string(), Message::Integer(count));
    
    // Update state
    let mut new_state = serde_json::Map::new();
    new_state.insert("count".to_string(), count.into());
    
    Ok(ActorResult {
        outputs,
        state: Some(serde_json::Value::Object(new_state)),
    })
}

actor_plugin!(
    metadata: metadata(),
    process: process_actor
);
```

### HTTP Request Actor

```rust
use reflow_wasm::*;
use anyhow::Result;
use std::collections::HashMap;

fn metadata() -> PluginMetadata {
    create_transform_metadata(
        "HttpFetcher",
        "Fetches data from HTTP endpoints",
        "String",  // URL input
        "Object"   // JSON response
    )
}

fn process_actor(context: ActorContext) -> Result<ActorResult> {
    if let Some(Message::String(url)) = context.payload.get("input") {
        // Note: HTTP requests must be handled by the host environment
        // This is a simplified example
        
        // For now, return a mock response
        let response = serde_json::json!({
            "url": url,
            "status": 200,
            "body": "Mock response"
        });
        
        return Ok(success_result("output", Message::Object(response)));
    }
    
    Ok(error_result("No URL provided"))
}

actor_plugin!(
    metadata: metadata(),
    process: process_actor
);
```

## Building for Other Languages

### Go Example

```go
package main

import (
    "github.com/extism/go-pdk"
)

//export get_metadata
func getMetadata() int32 {
    metadata := map[string]interface{}{
        "component": "GoActor",
        "description": "Actor written in Go",
        "inports": []map[string]interface{}{
            {
                "name": "input",
                "description": "Input value",
                "port_type": "Integer",
                "required": true,
            },
        },
        "outports": []map[string]interface{}{
            {
                "name": "output",
                "description": "Output value",
                "port_type": "Integer",
                "required": false,
            },
        },
    }
    
    return pdk.OutputJSON(metadata)
}

//export process
func process() int32 {
    var context map[string]interface{}
    err := pdk.InputJSON(&context)
    if err != nil {
        return 1
    }
    
    // Process the input
    payload := context["payload"].(map[string]interface{})
    if input, ok := payload["input"]; ok {
        value := int(input.(map[string]interface{})["data"].(float64))
        result := value * 2
        
        output := map[string]interface{}{
            "outputs": map[string]interface{}{
                "output": map[string]interface{}{
                    "type": "Integer",
                    "data": result,
                },
            },
        }
        
        return pdk.OutputJSON(output)
    }
    
    return 0
}

func main() {}
```

Build with:
```bash
tinygo build -o plugin.wasm -target wasi main.go
```

## Best Practices

1. **Error Handling**: Always return proper error messages using `error_result()` or the Error message type.

2. **Input Validation**: Validate all inputs before processing:
   ```rust
   if let Some(Message::Integer(value)) = context.payload.get("input") {
       if *value < 0 {
           return Ok(error_result("Input must be non-negative"));
       }
       // Process value...
   }
   ```

3. **State Management**: Keep state minimal and JSON-serializable.

4. **Performance**: 
   - Minimize allocations
   - Use `&str` instead of `String` where possible
   - Avoid complex computations in the plugin when possible

5. **Testing**: Test your plugins with the Extism CLI:
   ```bash
   extism call plugin.wasm process --input '{"payload":{"input":{"type":"Integer","data":42}}}'
   ```

## Integration with Reflow

Once built, your plugin can be used in Reflow workflows:

```json
{
  "nodes": [
    {
      "id": "multiplier",
      "component": "ExtismActor",
      "metadata": {
        "plugin_path": "./plugins/my_reflow_plugin.wasm",
        "entry_point": "process",
        "factor": 3
      }
    }
  ]
}
```

The plugin will be loaded and executed by the Reflow runtime, with full access to the actor system's features including state management, message passing, and configuration.