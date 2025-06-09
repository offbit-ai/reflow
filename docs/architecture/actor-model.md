# Actor Model

This document provides an in-depth look at how Reflow implements the Actor Model of computation.

## Introduction

The Actor Model is a mathematical model of concurrent computation that treats "actors" as the universal primitives of concurrent computation. In Reflow, actors are isolated computational units that communicate exclusively through message passing.

## Core Principles

### 1. Everything is an Actor

In Reflow's actor system:
- Data processing units are actors
- Message routers are actors
- Database connections are actors
- Network services are actors

### 2. Actors Communicate via Messages

```rust
// Messages are immutable and serializable
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

### 3. Actors Have Private State

```rust
pub trait ActorState: Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
    fn as_mut_any(&mut self) -> &mut dyn Any;
}

#[derive(Default, Debug, Clone)]
pub struct MemoryState(pub HashMap<String, Value>);
```

### 4. Actors Process Messages Sequentially

Each actor processes one message at a time, ensuring thread safety without locks.

## Actor Implementation

### Actor Trait

```rust
pub trait Actor: Send + Sync + 'static {
    /// Defines how the actor processes messages
    fn get_behavior(&self) -> ActorBehavior;
    
    /// Access to input ports
    fn get_inports(&self) -> Port;
    
    /// Access to output ports
    fn get_outports(&self) -> Port;
    
    /// Create the actor's execution process
    fn create_process(&self) -> Pin<Box<dyn Future<Output = ()> + 'static + Send>>;
    
    /// Load counting for backpressure (optional)
    fn load_count(&self) -> Arc<Mutex<ActorLoad>> {
        Arc::new(Mutex::new(ActorLoad::new(0)))
    }
}
```

### Actor Behavior

The behavior function defines how an actor responds to messages:

```rust
pub type ActorBehavior = Box<
    dyn Fn(ActorContext) -> Pin<Box<dyn Future<Output = Result<HashMap<String, Message>, anyhow::Error>> + Send + 'static>>
        + Send + Sync + 'static,
>;
```

### Actor Context

The context provides access to the actor's environment:

```rust
pub struct ActorContext {
    pub payload: ActorPayload,
    pub outports: Port,
    pub state: Arc<Mutex<dyn ActorState>>,
    pub config: HashMap<String, Value>,
    load: Arc<Mutex<ActorLoad>>,
}

impl ActorContext {
    pub fn get_state(&self) -> Arc<Mutex<dyn ActorState>>;
    pub fn get_config(&self) -> &HashMap<String, Value>;
    pub fn get_payload(&self) -> &ActorPayload;
    pub fn get_outports(&self) -> Port;
    pub fn done(&self);
}
```

## Actor Types

### 1. Native Actors

Written directly in Rust for maximum performance:

```rust
use reflow_network::actor::{Actor, ActorBehavior, ActorContext, Port, MemoryState};
use reflow_network::message::Message;
use std::collections::HashMap;

pub struct FilterActor {
    threshold: f64,
    inports: Port,
    outports: Port,
}

impl FilterActor {
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold,
            inports: flume::unbounded(),
            outports: flume::unbounded(),
        }
    }
}

impl Actor for FilterActor {
    fn get_behavior(&self) -> ActorBehavior {
        let threshold = self.threshold;
        
        Box::new(move |context: ActorContext| {
            Box::pin(async move {
                let payload = context.get_payload();
                let mut results = HashMap::new();
                
                if let Some(Message::Float(value)) = payload.get("input") {
                    if *value > threshold {
                        results.insert("output".to_string(), Message::Float(*value));
                    }
                }
                
                Ok(results)
            })
        })
    }
    
    fn get_inports(&self) -> Port { self.inports.clone() }
    fn get_outports(&self) -> Port { self.outports.clone() }
    
    fn create_process(&self) -> Pin<Box<dyn Future<Output = ()> + 'static + Send>> {
        // Implementation details...
        todo!()
    }
}
```

### 2. Script Actors

Execute scripts in various languages:

```rust
use reflow_script::{ScriptActor, ScriptConfig, ScriptRuntime, ScriptEnvironment};

// JavaScript Actor
let js_config = ScriptConfig {
    environment: ScriptEnvironment::SYSTEM,
    runtime: ScriptRuntime::JavaScript,
    source: include_bytes!("script.js").to_vec(),
    entry_point: "process".to_string(),
    packages: None,
};

let js_actor = ScriptActor::new(js_config);
```

### 3. Component Actors

Pre-built components from the library:

```rust
use reflow_components::flow_control::ConditionalActor;
use reflow_components::data_operations::MapActor;

let conditional = ConditionalActor::new(|msg| {
    if let Message::Integer(n) = msg {
        *n > 0
    } else {
        false
    }
});

let mapper = MapActor::new(|msg| {
    if let Message::Integer(n) = msg {
        Message::Integer(n * 2)
    } else {
        msg.clone()
    }
});
```

## Message Passing Semantics

### Asynchronous Messaging

Messages are sent asynchronously without blocking:

```rust
// Send message without waiting
outport.send_async(message).await?;

// Receive message when available
let message = inport.recv_async().await?;
```

### Message Ordering

- Messages between the same pair of actors maintain order
- No global ordering guarantees across different actor pairs
- Use synchronization actors for coordination when needed

### Message Delivery

- **At-most-once**: Messages may be lost but never duplicated
- **Best-effort**: System attempts delivery but doesn't guarantee it
- **Backpressure**: Slow consumers cause senders to block

## Actor Lifecycle Management

### Creation and Initialization

```rust
// Create actor
let actor = MyActor::new(config);

// Initialize ports and state
let inports = actor.get_inports();
let outports = actor.get_outports();

// Start actor process
let process = actor.create_process();
tokio::spawn(process);
```

### Message Processing Loop

```rust
pub fn create_process(&self) -> Pin<Box<dyn Future<Output = ()> + 'static + Send>> {
    let inports = self.get_inports();
    let behavior = self.get_behavior();
    let state = Arc::new(Mutex::new(MemoryState::default()));
    let outports = self.get_outports();
    
    Box::pin(async move {
        while let Ok(payload) = inports.1.recv_async().await {
            let context = ActorContext::new(
                payload,
                outports.clone(),
                state.clone(),
                HashMap::new(),
                Arc::new(Mutex::new(ActorLoad::new(0))),
            );
            
            match behavior(context).await {
                Ok(result) => {
                    if !result.is_empty() {
                        let _ = outports.0.send_async(result).await;
                    }
                },
                Err(e) => {
                    let error_msg = HashMap::from([
                        ("error".to_string(), Message::Error(e.to_string()))
                    ]);
                    let _ = outports.0.send_async(error_msg).await;
                }
            }
        }
    })
}
```

### Termination

Actors terminate when:
- Input ports are closed (no more messages)
- Explicit shutdown signal
- Unrecoverable error occurs

## State Management

### Actor State Types

```rust
// Simple memory state
let state = MemoryState::default();

// Custom state implementation
struct CounterState {
    count: AtomicU64,
}

impl ActorState for CounterState {
    fn as_any(&self) -> &dyn Any { self }
    fn as_mut_any(&mut self) -> &mut dyn Any { self }
}
```

### State Persistence

```rust
// Access state in behavior
fn get_behavior(&self) -> ActorBehavior {
    Box::new(|context: ActorContext| {
        Box::pin(async move {
            let state = context.get_state();
            let mut state_guard = state.lock();
            
            // Read/modify state
            if let Some(memory_state) = state_guard.as_mut_any().downcast_mut::<MemoryState>() {
                memory_state.insert("counter", serde_json::json!(42));
            }
            
            Ok(HashMap::new())
        })
    })
}
```

## Error Handling

### Actor-Level Errors

```rust
// Return error from behavior
Err(anyhow::anyhow!("Processing failed: {}", reason))

// Handle errors in message processing
match behavior(context).await {
    Ok(result) => send_result(result).await,
    Err(e) => send_error(e).await,
}
```

### Error Propagation

```rust
// Error message format
let error_message = HashMap::from([
    ("error".to_string(), Message::Error("Database connection failed".to_string())),
    ("code".to_string(), Message::Integer(500)),
    ("timestamp".to_string(), Message::String(Utc::now().to_rfc3339())),
]);
```

### Supervision Strategies

```rust
// Supervisor actor monitors children
struct SupervisorActor {
    children: Vec<ActorRef>,
    restart_policy: RestartPolicy,
}

enum RestartPolicy {
    OneForOne,    // Restart only failed actor
    OneForAll,    // Restart all actors
    RestForOne,   // Restart failed and subsequent actors
}
```

## Concurrency and Parallelism

### Actor Isolation

- Each actor runs in isolation
- No shared mutable state
- Communication only via messages
- Thread-safe by design

### Parallel Execution

```rust
// Multiple actors can run simultaneously
tokio::spawn(actor1.create_process());
tokio::spawn(actor2.create_process());
tokio::spawn(actor3.create_process());

// Actors on different CPU cores
let rt = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(num_cpus::get())
    .build()?;
```

### Load Balancing

```rust
// Round-robin message distribution
struct LoadBalancerActor {
    workers: Vec<Port>,
    current: AtomicUsize,
}

impl LoadBalancerActor {
    fn next_worker(&self) -> &Port {
        let index = self.current.fetch_add(1, Ordering::Relaxed) % self.workers.len();
        &self.workers[index]
    }
}
```

## Performance Considerations

### Memory Usage

- Actors have minimal overhead (~1KB per actor)
- Messages are reference-counted when possible
- State is lazily allocated

### Message Throughput

- Local messages: >1M messages/second
- Network messages: 10K-100K messages/second
- Batch processing for high throughput

### Backpressure Handling

```rust
// Check actor load before sending
let load = actor.load_count();
if load.lock().get() > MAX_LOAD {
    // Apply backpressure
    tokio::time::sleep(Duration::from_millis(10)).await;
}
```

## Advanced Patterns

### Actor Pooling

```rust
struct ActorPool<T: Actor> {
    actors: Vec<T>,
    distributor: LoadBalancerActor,
}

impl<T: Actor> ActorPool<T> {
    pub fn new(size: usize, factory: impl Fn() -> T) -> Self {
        let actors: Vec<T> = (0..size).map(|_| factory()).collect();
        // ... setup distributor
    }
}
```

### Hot Swapping

```rust
// Replace actor behavior without stopping
actor.update_behavior(new_behavior).await?;

// Migrate state to new actor version
let old_state = old_actor.get_state();
new_actor.set_state(old_state).await?;
```

### Circuit Breaker

```rust
struct CircuitBreakerActor {
    target: ActorRef,
    failure_count: AtomicU32,
    state: AtomicU8, // Open, Closed, HalfOpen
}

impl CircuitBreakerActor {
    fn should_allow_request(&self) -> bool {
        match self.state.load(Ordering::Relaxed) {
            0 => true,  // Closed
            1 => false, // Open
            2 => true,  // HalfOpen
            _ => false,
        }
    }
}
```

## Testing Actors

### Unit Testing

```rust
#[tokio::test]
async fn test_filter_actor() {
    let actor = FilterActor::new(5.0);
    let behavior = actor.get_behavior();
    
    // Create test context
    let payload = HashMap::from([
        ("input".to_string(), Message::Float(10.0))
    ]);
    
    let context = create_test_context(payload);
    let result = behavior(context).await.unwrap();
    
    assert!(result.contains_key("output"));
}
```

### Integration Testing

```rust
#[tokio::test]
async fn test_actor_pipeline() {
    let source = SourceActor::new();
    let filter = FilterActor::new(5.0);
    let sink = SinkActor::new();
    
    // Connect actors
    connect_actors(&source, &filter).await;
    connect_actors(&filter, &sink).await;
    
    // Start pipeline
    let handles = vec![
        tokio::spawn(source.create_process()),
        tokio::spawn(filter.create_process()),
        tokio::spawn(sink.create_process()),
    ];
    
    // Test data flow
    // ... assertions
}
```

## Best Practices

### Actor Design

1. **Keep actors small and focused** - Single responsibility principle
2. **Avoid blocking operations** - Use async/await for I/O
3. **Handle errors gracefully** - Don't let actors crash
4. **Design for failure** - Expect message loss and actor failures

### Message Design

1. **Keep messages immutable** - Never modify messages after sending
2. **Use appropriate message sizes** - Balance between batching and latency
3. **Include context** - Messages should carry enough information
4. **Handle malformed messages** - Validate input gracefully

### State Management

1. **Minimize state** - Less state means fewer bugs
2. **Make state serializable** - Enable persistence and distribution
3. **Avoid shared state** - Each actor owns its state
4. **Design for recovery** - State should be reconstructible

## Next Steps

- [Message Passing](./message-passing.md) - Detailed message system
- [Graph System](./graph-system.md) - Workflow composition
- [Creating Actors](../api/actors/creating-actors.md) - Practical guide
- [Performance Optimization](./performance-considerations.md) - Tuning guidelines
