# Event Types Reference

This reference covers the event types in Reflow's observability pipeline: low-level `NetworkEvent`s from the actor runtime, enriched `EngineEvent`s from the execution engine, and the ZIP events sent to Zeal IDE.

## EngineEvent Structure

All engine events share a common structure:

```rust
pub struct EngineEvent {
    pub workflow_id: String,
    pub execution_id: String,
    pub event_type: EngineEventType,
    pub timestamp: u64,
    pub data: serde_json::Value,
}
```

## EngineEventType

### Started

Emitted when an execution begins.

```rust
EngineEventType::Started
```

**ZIP mapping:** `ZipExecutionEvent::ExecutionStarted`

### NodeExecuting

Emitted when an actor begins processing. Generated from `NetworkEvent::ActorStarted`.

```rust
EngineEventType::NodeExecuting {
    node_id: String,     // Actor/node identifier
    component: String,   // Component type name
}
```

**ZIP mapping:** `ZipExecutionEvent::NodeExecuting`

### ActorCompleted

Emitted when an actor finishes successfully. Generated from `NetworkEvent::ActorCompleted`. The engine computes `duration_ms` by comparing the `ActorStarted` timestamp stored in a `HashMap<String, Instant>`.

```rust
EngineEventType::ActorCompleted {
    actor_id: String,
    component: String,
    duration_ms: Option<u64>,          // Time from ActorStarted → ActorCompleted
    output_size: Option<u64>,          // Serialized output size in bytes
    output_connections: Vec<String>,   // IDs of outbound connections
}
```

**ZIP mapping:** `ZipExecutionEvent::NodeCompleted` with `NodeCompletedOptions { duration, output_size }`

**Trace mapping:** `TraceEventType::Output` with `TraceData { size, data_type: "application/json", preview, duration }`

### ActorFailed

Emitted when an actor errors. Generated from `NetworkEvent::ActorFailed`.

```rust
EngineEventType::ActorFailed {
    actor_id: String,
    component: String,
    error: String,                     // Error message
    output_connections: Vec<String>,   // Outbound connections (for error routing)
}
```

**ZIP mapping:** `ZipExecutionEvent::NodeFailed` with `NodeError { message, code, stack }`

**Trace mapping:** `TraceEventType::Error` with `TraceError { message }`

### MessageSent

Emitted when data flows between actors. Generated from `NetworkEvent::MessageSent`.

```rust
EngineEventType::MessageSent {
    from_node: String,
    from_port: String,
    to_node: String,
    to_port: String,
    size: usize,    // Serialized message size in bytes
}
```

**ZIP mapping:** None (silently dropped)

**Trace mapping:** `TraceEventType::Output` with `TraceData { data_type: "message", size, preview: { to_node, to_port } }`

### NetworkIdle

Emitted when the network has no more messages to process. Used internally to trigger the `Completed` event.

```rust
EngineEventType::NetworkIdle
```

**ZIP mapping:** None

### Completed

Emitted when the execution finishes. Generated after `NetworkIdle` or `NetworkShutdown`. Includes aggregate statistics.

```rust
EngineEventType::Completed {
    duration_ms: u64,       // Total execution wall-clock time
    nodes_executed: u32,    // Total actors that ran
    nodes_failed: u32,      // Actors that failed
}
```

**ZIP mapping:** `ZipExecutionEvent::ExecutionCompleted` with:
```rust
ExecutionCompletedOptions {
    summary: Some(ExecutionSummary {
        success_count: nodes_executed - nodes_failed,
        error_count: nodes_failed,
        warning_count: 0,
    }),
}
```

### Failed

Emitted when the execution fails at the engine level (not an individual actor failure).

```rust
EngineEventType::Failed {
    error: String,
    duration_ms: Option<u64>,
}
```

**ZIP mapping:** `ZipExecutionEvent::ExecutionFailed` with:
```rust
ExecutionError { message, code: None, node_id: None }
ExecutionFailedOptions { duration }
```

## NetworkEvent (Source Events)

These are the raw events from `reflow_network` that the engine translates:

| NetworkEvent | Description |
|-------------|-------------|
| `ActorStarted { actor_id }` | Actor process began (records start time in HashMap) |
| `ActorCompleted { actor_id, output }` | Actor finished (triggers `EngineEventType::ActorCompleted`) |
| `ActorFailed { actor_id, error }` | Actor errored (triggers `EngineEventType::ActorFailed`) |
| `MessageSent { from_actor, from_port, to_actor, to_port, data }` | Data transferred between actors |
| `NetworkIdle` | No pending messages (triggers completion check) |
| `NetworkShutdown` | Network stopped |

## TraceEvent (Zeal TracesAPI)

Events submitted to Zeal's TracesAPI via HTTP:

```rust
pub struct TraceEvent {
    pub timestamp: i64,
    pub node_id: String,
    pub port_id: Option<String>,
    pub event_type: TraceEventType,   // Input, Output, Error
    pub data: TraceData,
    pub duration: Option<Duration>,
    pub metadata: Option<Value>,
    pub error: Option<TraceError>,
}

pub struct TraceData {
    pub size: usize,
    pub data_type: String,
    pub preview: Option<Value>,
    pub full_data: Option<Value>,
}
```

### TraceEventType

| Type | Used For |
|------|----------|
| `Input` | `NodeExecuting` — actor began processing |
| `Output` | `ActorCompleted`, `MessageSent` — data produced |
| `Error` | `ActorFailed` — error occurred |

## ZipExecutionEvent (Zeal WebSocket)

Events sent over the ZIP WebSocket to Zeal in real-time:

| Event | Description | Key Fields |
|-------|-------------|------------|
| `ExecutionStarted` | Workflow began | workflow_id, execution_id |
| `NodeExecuting` | Actor began processing | workflow_id, node_id, input_connections |
| `NodeCompleted` | Actor finished | workflow_id, node_id, output_connections, duration, output_size |
| `NodeFailed` | Actor errored | workflow_id, node_id, error message |
| `ExecutionCompleted` | Workflow finished | duration, nodes_executed, summary |
| `ExecutionFailed` | Workflow failed | error, duration |

All ZIP events are created using `zeal-sdk` helper functions (`create_execution_started_event`, `create_node_completed_event`, etc.) and serialized as JSON text frames.

## Event Lifecycle Example

For a workflow with two actors (A → B):

```
1. EngineEventType::Started
2. EngineEventType::NodeExecuting { node_id: "A", component: "tpl_http_request" }
3. EngineEventType::ActorCompleted { actor_id: "A", duration_ms: Some(150), output_size: Some(2048) }
4. EngineEventType::MessageSent { from: "A", to: "B", size: 2048 }
5. EngineEventType::NodeExecuting { node_id: "B", component: "tpl_data_transformer" }
6. EngineEventType::ActorCompleted { actor_id: "B", duration_ms: Some(10), output_size: Some(512) }
7. EngineEventType::NetworkIdle
8. EngineEventType::Completed { duration_ms: 165, nodes_executed: 2, nodes_failed: 0 }
```

This generates:
- 6 ZIP WebSocket events (Started, NodeExecuting x2, NodeCompleted x2, ExecutionCompleted)
- 6 TraceEvents (Input x2, Output x3 including MessageSent, Output for completion)
- 1 trace session begin + 1 trace session complete with summary

## Next Steps

- [Observability Architecture](./architecture.md) - Pipeline architecture and configuration
- [Data Flow Tracing](./data-flow-tracing.md) - Message-level data flow tracking
- [Zeal IDE Integration](../integration/zeal-ide.md) - ZIP session and template registration
