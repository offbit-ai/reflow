# WASM Plugin Examples

This directory contains example plugins demonstrating how to use the Reflow WASM SDK.

## Examples

### 1. multiply_actor
A simple actor that multiplies input values by a configurable factor. Demonstrates:
- Basic message processing
- Configuration access
- State management (operation count tracking)
- Error handling

### 2. counter_actor
A stateful counter actor with multiple operations. Demonstrates:
- Complex state management
- Multiple input ports
- Constraint handling (min/max values)
- Rich output data

## Building the Examples

Each example can be built using its build script:

```bash
cd multiply_actor
./build.sh
```

Or manually:

```bash
cd multiply_actor
cargo build --release --target wasm32-unknown-unknown
```

## Testing with ScriptActor

Here's how to use these plugins with Reflow's ScriptActor:

```rust
use reflow_script::{ScriptActor, ScriptConfig, ScriptEnvironment, ScriptRuntime};
use reflow_actor::{Actor, ActorConfig};
use std::collections::HashMap;

// Load the plugin
let wasm_bytes = std::fs::read("multiply_actor/build/multiply_actor.wasm")?;

// Create ScriptConfig
let config = ScriptConfig {
    environment: ScriptEnvironment::SYSTEM,
    runtime: ScriptRuntime::Extism,
    source: wasm_bytes,
    entry_point: "process".to_string(),
    packages: None,
};

// Create the actor
let actor = ScriptActor::new(config);

// Configure the actor
let mut metadata = HashMap::new();
metadata.insert("factor".to_string(), serde_json::json!(2.5));

let actor_config = ActorConfig::from_node(reflow_graph::types::GraphNode {
    id: "multiplier".to_string(),
    component: "MultiplyActor".to_string(),
    metadata: Some(metadata),
})?;

// Start the actor
let process = actor.create_process(actor_config, None);
tokio::spawn(process);

// Send messages
let inports = actor.get_inports();
let outports = actor.get_outports();

let mut payload = HashMap::new();
payload.insert("value".to_string(), Message::Integer(10));

inports.0.send_async(payload).await?;
let result = outports.1.recv_async().await?;

println!("Result: {:?}", result["result"]); // Should be Integer(25)
```

## Plugin Development Tips

1. **Use the SDK helpers**: The SDK provides many helper functions to simplify common tasks.

2. **Test locally**: Use the Extism CLI to test your plugins before integrating with Reflow:
   ```bash
   extism call multiply_actor.wasm process --input '{"payload":{"value":{"type":"Integer","data":10}},"config":{"factor":2},"state":{}}'
   ```

3. **Handle errors gracefully**: Always validate inputs and return appropriate error messages.

4. **Keep state minimal**: State is serialized/deserialized on each call, so keep it lightweight.

5. **Document your ports**: Use clear descriptions in your PluginMetadata.

## Integration in Reflow Graphs

Once built, these plugins can be used in Reflow workflow definitions:

```json
{
  "id": "wasm-example",
  "nodes": [
    {
      "id": "source",
      "component": "DataSource",
      "metadata": {
        "data": [1, 2, 3, 4, 5]
      }
    },
    {
      "id": "multiplier",
      "component": "ExtismActor",
      "metadata": {
        "runtime": "Extism",
        "source": "./plugins/multiply_actor.wasm",
        "entry_point": "process",
        "factor": 3
      }
    },
    {
      "id": "sink",
      "component": "Logger"
    }
  ],
  "edges": [
    {
      "from": "source",
      "from_port": "output",
      "to": "multiplier",
      "to_port": "value"
    },
    {
      "from": "multiplier",
      "from_port": "result",
      "to": "sink",
      "to_port": "input"
    }
  ]
}
```