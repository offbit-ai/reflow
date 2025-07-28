# Reflow WASM Plugin Integration Guide

This guide explains how the WASM plugin system integrates with Reflow's actor system.

## Architecture Overview

```
┌─────────────────────────┐     ┌──────────────────────┐
│   WASM Plugin (SDK)     │     │   Reflow Host       │
├─────────────────────────┤     ├──────────────────────┤
│ • reflow_wasm crate     │     │ • ScriptActor       │
│ • Extism PDK            │────▶│ • ExtismEngine      │
│ • Plugin code           │     │ • Host functions    │
└─────────────────────────┘     └──────────────────────┘
```

## How It Works

### 1. Plugin Development (reflow_wasm)

Developers use the `reflow_wasm` SDK to create WASM plugins:

```rust
use reflow_wasm::*;

// Define metadata
fn metadata() -> PluginMetadata {
    PluginMetadata {
        component: "MyActor".to_string(),
        // ... ports and config
    }
}

// Implement behavior
fn process(context: ActorContext) -> Result<ActorResult> {
    // Process messages
}

// Register plugin
actor_plugin!(
    metadata: metadata(),
    process: process
);
```

### 2. Host Loading (reflow_script)

The host loads plugins using `ScriptActor` with `ExtismEngine`:

```rust
use reflow_script::{ScriptActor, ScriptConfig, ScriptRuntime};

let config = ScriptConfig {
    runtime: ScriptRuntime::Extism,
    source: wasm_bytes,
    entry_point: "process".to_string(),
    // ...
};

let actor = ScriptActor::new(config);
```

### 3. Message Flow

1. **Incoming Message**: 
   - Reflow sends message to ScriptActor
   - ScriptActor creates ScriptContext
   - ExtismEngine converts to plugin format

2. **Plugin Processing**:
   - Plugin receives ActorContext (JSON)
   - Processes using SDK types
   - Returns ActorResult (JSON)

3. **Outgoing Message**:
   - ExtismEngine converts result
   - ScriptActor sends to Reflow network

### 4. State Management

State is managed through host functions:

```rust
// In plugin
let current_value = context.state.get("counter");
// Process...
result.state = Some(updated_state);

// Host maintains state between calls
```

## Host Functions

The ExtismEngine provides these host functions:

- `__send_output`: Send messages to output ports
- `__get_state`: Retrieve actor state
- `__set_state`: Update actor state

## Message Type Mapping

| Plugin Type | Reflow Type |
|-------------|-------------|
| `Message::Integer(i64)` | `reflow_actor::message::Message::Integer(i64)` |
| `Message::String(String)` | `reflow_actor::message::Message::String(Arc<String>)` |
| `Message::Object(Value)` | `reflow_actor::message::Message::Object(Arc<EncodableValue>)` |
| etc. | etc. |

## Configuration Flow

1. **Graph Definition**:
   ```json
   {
     "id": "my-actor",
     "component": "ExtismActor",
     "metadata": {
       "source": "./plugin.wasm",
       "factor": 2
     }
   }
   ```

2. **ActorConfig Creation**:
   - Resolves environment variables
   - Merges metadata
   - Passes to plugin

3. **Plugin Access**:
   ```rust
   let factor = context.config.get_number("factor");
   ```

## Testing Plugins

### Unit Testing (Plugin Side)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_process() {
        let context = ActorContext {
            payload: HashMap::from([
                ("input", Message::Integer(5))
            ]),
            // ...
        };
        
        let result = process(context).unwrap();
        assert_eq!(result.outputs["output"], Message::Integer(10));
    }
}
```

### Integration Testing (Host Side)

```rust
#[tokio::test]
async fn test_plugin() {
    let wasm_bytes = std::fs::read("plugin.wasm").unwrap();
    let config = ScriptConfig {
        runtime: ScriptRuntime::Extism,
        source: wasm_bytes,
        // ...
    };
    
    let actor = ScriptActor::new(config);
    // Test actor behavior
}
```

## Performance Considerations

1. **Message Serialization**: Messages are serialized to JSON for plugin calls
2. **State Persistence**: State is maintained by the host between calls
3. **Memory Management**: WASM memory is isolated and managed by Extism

## Security

1. **Sandboxing**: Plugins run in WASM sandbox
2. **Resource Limits**: Can be configured via Extism
3. **Capability-based**: Only allowed host functions are accessible

## Debugging

1. **Logging**: Use stderr in plugins (captured by host)
2. **Extism CLI**: Test plugins independently
3. **State Inspection**: Host can log state changes

## Example Workflow

1. Developer creates plugin using `reflow_wasm` SDK
2. Builds to WASM: `cargo build --target wasm32-unknown-unknown`
3. Deploys WASM file with Reflow application
4. Configures graph to use ExtismActor
5. Reflow loads and executes plugin as part of workflow

This architecture enables multi-language actor development while maintaining Reflow's actor model semantics.