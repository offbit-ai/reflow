# WebAssembly Runtime

The WebAssembly runtime in Reflow enables execution of WASM-based actors using the Extism plugin system. This provides a secure, sandboxed environment for running plugins written in any language that compiles to WebAssembly.

## Overview

Reflow's WASM runtime is built on [Extism](https://extism.org/), a cross-language framework for building plugin systems. It allows you to:

- Write actors in languages like Rust, Go, C/C++, Zig, and more
- Run plugins in a secure, sandboxed environment
- Share state between plugin invocations
- Communicate with the host system through well-defined interfaces

## Architecture

```
┌─────────────────────────────────────────────┐
│              ScriptActor                    │
│  ┌─────────────────────────────────────┐   │
│  │         ExtismEngine                 │   │
│  │  ┌─────────────────────────────┐    │   │
│  │  │    Extism Plugin Host       │    │   │
│  │  │  ┌───────────────────┐      │    │   │
│  │  │  │   WASM Plugin     │      │    │   │
│  │  │  │  ┌─────────────┐  │      │    │   │
│  │  │  │  │ Actor Logic │  │      │    │   │
│  │  │  │  └─────────────┘  │      │    │   │
│  │  │  └───────────────────┘      │    │   │
│  │  └─────────────────────────────┘    │   │
│  └─────────────────────────────────────┘   │
└─────────────────────────────────────────────┘
```

## Plugin SDK

Reflow provides a Rust SDK (`reflow_wasm`) for building WASM plugins:

```rust
use reflow_wasm::*;
use std::collections::HashMap;

// Define plugin metadata
fn metadata() -> PluginMetadata {
    PluginMetadata {
        component: "MyActor".to_string(),
        description: "Example WASM actor".to_string(),
        inports: vec![
            port_def!("input", "Input port", "Integer", required),
        ],
        outports: vec![
            port_def!("output", "Output port", "Integer"),
        ],
        config_schema: None,
    }
}

// Implement actor behavior
fn process_actor(context: ActorContext) -> Result<ActorResult, Box<dyn std::error::Error>> {
    let mut outputs = HashMap::new();
    
    if let Some(Message::Integer(value)) = context.payload.get("input") {
        outputs.insert("output".to_string(), Message::Integer(value * 2));
    }
    
    Ok(ActorResult {
        outputs,
        state: None,
    })
}

// Register the plugin
actor_plugin!(
    metadata: metadata(),
    process: process_actor
);
```

## Host Functions

The WASM runtime provides several host functions that plugins can call:

### State Management
- `__get_state(key: string) -> value` - Retrieve a value from actor state
- `__set_state(key: string, value: any)` - Store a value in actor state

### Output
- `__send_output(outputs: HashMap<string, Message>)` - Send messages to output ports

## Message Types

The runtime supports all Reflow message types:

```rust
pub enum Message {
    Flow,                          // Control flow signal
    Event(Value),                  // Event with data
    Boolean(bool),                 // Boolean value
    Integer(i64),                  // 64-bit integer
    Float(f64),                    // 64-bit float
    String(String),                // UTF-8 string
    Object(Value),                 // JSON object
    Array(Vec<Value>),            // Array of values
    Stream(Vec<u8>),              // Binary data
    Optional(Option<Box<Value>>),  // Optional value
    Any(Value),                    // Any JSON value
    Error(String),                 // Error message
}
```

## Configuration

WASM actors are configured through the `ScriptConfig`:

```rust
let config = ScriptConfig {
    environment: ScriptEnvironment::SYSTEM,
    runtime: ScriptRuntime::Extism,
    source: wasm_bytes,  // Compiled WASM binary
    entry_point: "process".to_string(),
    packages: None,
};
```

## Security

The WASM runtime provides several security features:

1. **Sandboxing**: Plugins run in isolated WASM sandboxes
2. **Resource Limits**: Memory and execution time can be limited
3. **Host Function Access**: Plugins can only call explicitly provided host functions
4. **No Direct System Access**: Plugins cannot access the file system or network directly

## Performance Considerations

- **Startup Cost**: WASM modules have some initialization overhead
- **Memory Overhead**: Each plugin instance requires its own memory space
- **Cross-Boundary Calls**: Data serialization between host and plugin has a cost
- **Optimization**: Use release builds and wasm-opt for best performance

## Example: Stateful Counter

```rust
use reflow_wasm::*;
use std::collections::HashMap;

fn metadata() -> PluginMetadata {
    PluginMetadata {
        component: "Counter".to_string(),
        description: "Stateful counter actor".to_string(),
        inports: vec![
            port_def!("increment", "Increment counter", "Flow"),
            port_def!("decrement", "Decrement counter", "Flow"),
            port_def!("reset", "Reset counter", "Flow"),
        ],
        outports: vec![
            port_def!("count", "Current count", "Integer"),
        ],
        config_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "initial_value": {
                    "type": "integer",
                    "default": 0
                }
            }
        })),
    }
}

fn process_actor(context: ActorContext) -> Result<ActorResult, Box<dyn std::error::Error>> {
    let mut outputs = HashMap::new();
    
    // Get current count from state
    let mut count = context.state
        .get("count")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| {
            context.config.get_integer("initial_value").unwrap_or(0)
        });
    
    // Process inputs
    if context.payload.contains_key("increment") {
        count += 1;
    } else if context.payload.contains_key("decrement") {
        count -= 1;
    } else if context.payload.contains_key("reset") {
        count = context.config.get_integer("initial_value").unwrap_or(0);
    }
    
    // Output current count
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

## Debugging

To debug WASM plugins:

1. Use `println!` or `eprintln!` in your plugin code (output goes to host stderr)
2. Use the Extism CLI for testing: `extism call plugin.wasm function_name --input '...'`
3. Enable debug logging in the host: `RUST_LOG=reflow_script=debug`

## See Also

- [Module Loading](./modules.md) - How WASM modules are loaded and managed
- [Memory Management](./memory.md) - Memory allocation and limits
- [Plugin Development Guide](../../guides/wasm-plugin-development.md) - Step-by-step plugin creation