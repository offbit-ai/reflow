# Basic Concepts

This guide introduces the fundamental concepts of Reflow's actor-based workflow engine.

## Core Concepts

### Actors

**Actors** are the building blocks of Reflow workflows. Each actor is an isolated unit of computation that:

- Processes incoming messages
- Maintains its own state
- Communicates only through message passing
- Runs concurrently with other actors

```rust
// Example: Simple actor that doubles numbers (using actor macro)
use std::collections::HashMap;
use reflow_network::{
    actor::ActorContext,
    message::Message,
};
use actor_macro::actor;

#[actor(
    DoublerActor,
    inports::<100>(number),
    outports::<50>(result)
)]
async fn doubler_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    
    if let Some(Message::Integer(n)) = payload.get("number") {
        Ok([
            ("result".to_owned(), Message::integer(n * 2))
        ].into())
    } else {
        Err(anyhow::anyhow!("Expected integer input"))
    }
}

// Alternative: Manual implementation
use reflow_network::actor::{Actor, ActorBehavior, Port, ActorLoad};
use parking_lot::Mutex;
use std::sync::Arc;

pub struct ManualDoublerActor {
    inports: Port,
    outports: Port,
    load: Arc<Mutex<ActorLoad>>,
}

impl ManualDoublerActor {
    pub fn new() -> Self {
        Self {
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            load: Arc::new(Mutex::new(ActorLoad::new(0))),
        }
    }
}

impl Actor for ManualDoublerActor {
    fn get_behavior(&self) -> ActorBehavior {
        Box::new(|context: ActorContext| {
            Box::pin(async move {
                let payload = context.get_payload();
                if let Some(Message::Integer(n)) = payload.get("number") {
                    Ok([
                        ("result".to_owned(), Message::Integer(n * 2))
                    ].into())
                } else {
                    Err(anyhow::anyhow!("Expected integer input"))
                }
            })
        })
    }
    
    fn get_inports(&self) -> Port { self.inports.clone() }
    fn get_outports(&self) -> Port { self.outports.clone() }
    fn load_count(&self) -> Arc<Mutex<ActorLoad>> { self.load.clone() }
    
    fn create_process(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'static + Send>> {
        // Process creation implementation...
        todo!("See creating-actors.md for complete implementation")
    }
}
```

### Messages

**Messages** are the data that flows between actors. Reflow supports various message types:

```rust
pub enum Message {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<Message>),
    Object(HashMap<String, Message>),
    Binary(Vec<u8>),
    Null,
    Error(String),
}
```

### Ports

**Ports** are the communication channels between actors:

- **Input ports (inports)**: Receive messages from other actors
- **Output ports (outports)**: Send messages to other actors

```
┌─────────────┐
│    Actor    │
│  ┌───────┐  │
│  │ Logic │  │
│  └───────┘  │
│             │
│ in1 ──────→ │ ──────→ out1
│ in2 ──────→ │ ──────→ out2
└─────────────┘
```

### Workflows (Graphs)

**Workflows** are directed graphs of connected actors that define:

- Data flow between actors
- Processing logic and transformations
- Execution order and dependencies

```
┌─────────┐    ┌─────────┐    ┌─────────┐
│ Source  │───▶│Transform│───▶│  Sink   │
│ Actor   │    │ Actor   │    │ Actor   │
└─────────┘    └─────────┘    └─────────┘
```

### Actor State

Each actor can maintain its own **state** that persists between message processing:

```rust
// Example: Counter actor with state (using actor macro)
use reflow_network::actor::MemoryState;

#[actor(
    CounterActor,
    state(MemoryState),
    inports::<100>(increment),
    outports::<50>(count)
)]
async fn counter_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    
    let mut state_guard = state.lock();
    let memory_state = state_guard
        .as_mut_any()
        .downcast_mut::<MemoryState>()
        .expect("Expected MemoryState");
    
    // Initialize state if needed
    if !memory_state.contains_key("count") {
        memory_state.insert("count", serde_json::json!(0));
    }
    
    // Get current count
    let current_count = memory_state.get("count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    
    // Increment by 1 or by specified amount
    let increment_by = if let Some(Message::Integer(amount)) = payload.get("increment") {
        *amount
    } else {
        1 // Default increment
    };
    
    let new_count = current_count + increment_by;
    
    // Update state
    memory_state.insert("count", serde_json::json!(new_count));
    
    Ok([
        ("count".to_owned(), Message::Integer(new_count))
    ].into())
}
```

## Actor Types

### Native Actors (Rust)

Built directly in Rust for maximum performance:

```rust
struct ProcessorActor {
    // Implementation in Rust
}
```

### Script Actors

Execute scripts in various languages:

#### JavaScript/TypeScript (Deno)
```javascript
// JavaScript actor function
function process(inputs, context) {
    const data = inputs.data;
    return { result: data.toUpperCase() };
}
```

#### Python
```python
# Python actor script
import numpy as np

inputs = Context.get_inputs()
data = np.array(inputs["data"])
__return_value = data.sum()
```

#### WebAssembly
```rust
// WASM actor (compiled from Rust, C++, etc.)
#[no_mangle]
pub extern "C" fn process(input: &str) -> String {
    // Process input and return result
    format!("Processed: {}", input)
}
```

## Message Passing Patterns

### Point-to-Point
One actor sends to one specific actor:
```
Actor A ───────▶ Actor B
```

### Broadcast
One actor sends to multiple actors:
```
        ┌─────▶ Actor B
Actor A ┤
        └─────▶ Actor C
```

### Collect/Merge
Multiple actors send to one actor:
```
Actor A ┐
        ├─────▶ Actor C
Actor B ┘
```

## Concurrency Model

### Actor Isolation
- Each actor runs in its own execution context
- No shared memory between actors
- Thread-safe by design

### Message Processing
- Actors process messages asynchronously
- Messages are queued for processing
- Backpressure handling prevents overflow

### Parallelism
- Multiple actors can run simultaneously
- Work is distributed across available CPU cores
- Network can span multiple machines

## Error Handling

### Actor-Level Errors
```rust
// Errors are returned as Error messages
Err(anyhow::anyhow!("Processing failed"))
```

### Network-Level Errors
```rust
// Error propagation through the network
HashMap::from([
    ("error".to_string(), Message::Error("Network timeout".to_string()))
])
```

### Recovery Patterns
- Dead letter queues for failed messages
- Circuit breakers for failing actors
- Supervisor actors for monitoring

## Lifecycle Management

### Actor Creation
```rust
let actor = MyActor::new(config);
let process = actor.create_process();
tokio::spawn(process);
```

### Actor Termination
```rust
// Graceful shutdown
drop(inports); // Closes input channels
// Actor completes current message and exits
```

### State Persistence
```rust
// State can be persisted and restored
let state = actor.get_state();
// Serialize state for persistence
```

## Advanced Concepts

### Hot Code Reloading
- Script actors can be updated without stopping the workflow
- State preservation during updates

### Multi-tenancy
- Isolated workspaces for different users/projects
- Resource quotas and permissions

### Distributed Execution
- Actors can run on different machines
- Network-transparent message passing

## Best Practices

### Actor Design
- Keep actors small and focused
- Avoid blocking operations in actor logic
- Use async/await for I/O operations

### Message Design
- Use typed messages when possible
- Keep messages small and serializable
- Include error context in error messages

### Workflow Design
- Design for failure (circuit breakers, timeouts)
- Monitor actor performance and health
- Use appropriate parallelism levels

## Next Steps

Now that you understand the basic concepts:

1. **Set up development**: [Development Setup](./development-setup.md)
2. **Create your first workflow**: [First Workflow](./first-workflow.md)
3. **Learn about specific actors**: [Actor API](../api/actors/creating-actors.md)
4. **Explore scripting**: [JavaScript Runtime](../scripting/javascript/deno-runtime.md)

## Further Reading

- [Actor Model Theory](../architecture/actor-model.md)
- [Message Passing Details](../architecture/message-passing.md)
- [Performance Considerations](../architecture/performance-considerations.md)
