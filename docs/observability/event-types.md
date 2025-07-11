# Event Types Reference

The Reflow observability framework captures comprehensive trace events that provide deep insights into actor network behavior. This reference covers all available event types, their structure, and usage patterns.

## Core Event Structure

All trace events share a common structure defined in `TraceEvent`:

```rust
pub struct TraceEvent {
    pub event_id: EventId,           // Unique event identifier
    pub timestamp: DateTime<Utc>,    // UTC timestamp
    pub event_type: TraceEventType,  // Event type (see below)
    pub actor_id: String,            // Source actor identifier
    pub data: TraceEventData,        // Event-specific data
    pub causality: CausalityInfo,    // Causality tracking
}
```

## Actor Lifecycle Events

### ActorCreated

Triggered when an actor instance is created.

```rust
TraceEventType::ActorCreated
```

**When Generated**: 
- Actor registration in network
- Dynamic actor instantiation
- Actor factory creation

**Event Data**:
```rust
TraceEventData {
    port: None,
    message: None,
    state_diff: None,
    error: None,
    performance_metrics: PerformanceMetrics::default(),
    custom_attributes: HashMap::from([
        ("actor_type", json!("DataProcessor")),
        ("config", json!({"timeout": 5000})),
        ("instance_id", json!("proc_001")),
    ]),
}
```

**Example**:
```rust
if let Some(tracing) = global_tracing() {
    tracing.trace_actor_created("data_processor").await?;
}
```

### ActorStarted

Triggered when an actor begins execution.

```rust
TraceEventType::ActorStarted
```

**When Generated**:
- Actor process startup
- Actor behavior initialization
- Resource allocation completion

**Event Data**: Includes initialization metrics and startup configuration.

### ActorCompleted

Triggered when an actor successfully completes execution.

```rust
TraceEventType::ActorCompleted
```

**When Generated**:
- Successful actor termination
- Graceful shutdown completion
- Task completion

**Event Data**:
```rust
TraceEventData {
    performance_metrics: PerformanceMetrics {
        execution_time_ns: 150_000_000,  // 150ms
        memory_usage_bytes: 1024,
        cpu_usage_percent: 15.5,
        queue_depth: 0,
        throughput_msgs_per_sec: 100.0,
    },
    custom_attributes: HashMap::from([
        ("exit_code", json!(0)),
        ("processed_messages", json!(42)),
    ]),
    ..Default::default()
}
```

### ActorFailed

Triggered when an actor encounters an error.

```rust
TraceEventType::ActorFailed
```

**When Generated**:
- Unhandled exceptions
- Resource exhaustion
- Configuration errors
- Network failures

**Event Data**:
```rust
TraceEventData {
    error: Some("Connection timeout: Failed to connect to database".to_string()),
    performance_metrics: PerformanceMetrics {
        execution_time_ns: 5_000_000_000, // 5 seconds before timeout
        memory_usage_bytes: 2048,
        cpu_usage_percent: 95.0,
        queue_depth: 10,
        throughput_msgs_per_sec: 0.0,
    },
    custom_attributes: HashMap::from([
        ("error_code", json!("TIMEOUT")),
        ("retry_count", json!(3)),
        ("last_successful_operation", json!("database_query")),
    ]),
    ..Default::default()
}
```

## Communication Events

### MessageSent

Triggered when an actor sends a message.

```rust
TraceEventType::MessageSent
```

**When Generated**:
- Message transmission to ports
- Inter-actor communication
- Network message dispatch

**Event Data**:
```rust
TraceEventData {
    port: Some("output".to_string()),
    message: Some(MessageSnapshot {
        message_type: "ProcessedData".to_string(),
        size_bytes: 1024,
        checksum: "sha256:abc123...".to_string(),
        serialized_data: vec![], // Optional: actual message data
    }),
    performance_metrics: PerformanceMetrics {
        execution_time_ns: 1_000_000, // 1ms serialization time
        ..Default::default()
    },
    ..Default::default()
}
```

### MessageReceived

Triggered when an actor receives a message.

```rust
TraceEventType::MessageReceived
```

**When Generated**:
- Message reception on ports
- Queue processing
- Message deserialization

**Event Data**: Similar to `MessageSent` but from receiver perspective.

### DataFlow (NEW)

Automatically triggered when data flows between connected actors.

```rust
TraceEventType::DataFlow {
    to_actor: String,
    to_port: String,
}
```

**When Generated**:
- Connector-level message transmission
- Data pipeline operations
- Cross-actor data transfer

**Event Data**:
```rust
TraceEventData {
    port: Some("output".to_string()), // Source port
    message: Some(MessageSnapshot {
        message_type: "SensorReading".to_string(),
        size_bytes: 256,
        checksum: "sha256:def456...".to_string(),
        serialized_data: vec![],
    }),
    performance_metrics: PerformanceMetrics {
        execution_time_ns: 500_000, // 0.5ms transfer time
        queue_depth: 5, // Current queue depth at destination
        throughput_msgs_per_sec: 1000.0,
        ..Default::default()
    },
    custom_attributes: HashMap::from([
        ("source_actor", json!("sensor_reader")),
        ("source_port", json!("output")),
        ("destination_actor", json!("data_validator")),
        ("destination_port", json!("input")),
        ("transfer_protocol", json!("memory")),
    ]),
    ..Default::default()
}
```

**Usage**:
```rust
// Automatic - no manual code needed
// Generated when messages flow through connectors

// Manual usage for custom connectors:
tracing.trace_data_flow(
    "source_actor", "output_port",
    "dest_actor", "input_port", 
    "CustomMessage", 512
).await?;
```

## State Management Events

### StateChanged

Triggered when an actor's state is modified.

```rust
TraceEventType::StateChanged
```

**When Generated**:
- State updates
- Configuration changes
- Memory state modifications

**Event Data**:
```rust
TraceEventData {
    state_diff: Some(StateDiff {
        before: Some(previous_state_bytes),
        after: current_state_bytes,
        diff_type: StateDiffType::Incremental,
    }),
    performance_metrics: PerformanceMetrics {
        execution_time_ns: 2_000_000, // 2ms state update time
        memory_usage_bytes: 4096, // Memory used by state
        ..Default::default()
    },
    custom_attributes: HashMap::from([
        ("state_version", json!(42)),
        ("state_size_bytes", json!(4096)),
        ("changed_fields", json!(["counter", "last_update"])),
    ]),
    ..Default::default()
}
```

## Network Events

### PortConnected

Triggered when actor ports are connected.

```rust
TraceEventType::PortConnected
```

**Event Data**:
```rust
TraceEventData {
    port: Some("output".to_string()),
    custom_attributes: HashMap::from([
        ("connected_to_actor", json!("downstream_processor")),
        ("connected_to_port", json!("input")),
        ("connection_type", json!("memory_channel")),
        ("buffer_size", json!(1000)),
    ]),
    ..Default::default()
}
```

### PortDisconnected

Triggered when actor ports are disconnected.

```rust
TraceEventType::PortDisconnected
```

**Event Data**: Similar to `PortConnected` but for disconnection events.

### NetworkEvent

Triggered for network-level operations.

```rust
TraceEventType::NetworkEvent
```

**When Generated**:
- Network startup/shutdown
- Actor registration/deregistration
- Distributed network operations
- Load balancing events

**Event Data**:
```rust
TraceEventData {
    custom_attributes: HashMap::from([
        ("event_subtype", json!("actor_registered")),
        ("network_id", json!("production_cluster")),
        ("node_count", json!(5)),
        ("total_actors", json!(150)),
    ]),
    ..Default::default()
}
```

## Performance Metrics

All events include performance metrics:

```rust
pub struct PerformanceMetrics {
    pub execution_time_ns: u64,      // Nanoseconds
    pub memory_usage_bytes: usize,   // Bytes
    pub cpu_usage_percent: f32,      // 0.0 - 100.0
    pub queue_depth: usize,          // Messages in queue
    pub throughput_msgs_per_sec: f64, // Messages per second
}
```

### Interpreting Metrics

- **execution_time_ns**: Time spent on the operation
- **memory_usage_bytes**: Memory footprint at time of event
- **cpu_usage_percent**: CPU utilization during operation
- **queue_depth**: Number of pending messages
- **throughput_msgs_per_sec**: Recent message processing rate

## Causality Information

Events maintain causality chains for dependency tracking:

```rust
pub struct CausalityInfo {
    pub parent_event_id: Option<EventId>,    // Direct parent event
    pub root_cause_event_id: EventId,        // Root cause in chain
    pub dependency_chain: Vec<EventId>,      // Full dependency chain
    pub span_id: String,                     // Distributed tracing span
}
```

### Building Causality Chains

```rust
// Events can reference parent events
let parent_event_id = previous_event.event_id;

let child_event = TraceEvent {
    causality: CausalityInfo {
        parent_event_id: Some(parent_event_id),
        root_cause_event_id: original_trigger_event_id,
        dependency_chain: vec![original_trigger_event_id, parent_event_id],
        span_id: distributed_span_id,
    },
    ..Default::default()
};
```

## Custom Events

Create custom event types for domain-specific tracing:

```rust
// Custom event type
TraceEventType::Custom("deployment_started".to_string())

// Create custom event
let custom_event = TraceEvent {
    event_type: TraceEventType::Custom("model_training_completed".to_string()),
    actor_id: "ml_trainer".to_string(),
    data: TraceEventData {
        custom_attributes: HashMap::from([
            ("model_type", json!("neural_network")),
            ("training_epochs", json!(100)),
            ("accuracy", json!(0.95)),
            ("model_size_mb", json!(25.4)),
            ("training_time_hours", json!(3.5)),
        ]),
        performance_metrics: PerformanceMetrics {
            execution_time_ns: 12_600_000_000_000, // 3.5 hours in ns
            memory_usage_bytes: 1_073_741_824, // 1GB
            cpu_usage_percent: 85.0,
            ..Default::default()
        },
        ..Default::default()
    },
    ..Default::default()
};
```

## Event Filtering

Filter events by type for focused analysis:

```rust
// Subscribe to specific event types
let filters = SubscriptionFilters {
    flow_ids: Some(vec![FlowId::new("production_pipeline")]),
    actor_ids: Some(vec!["data_processor".to_string()]),
    event_types: Some(vec![
        TraceEventType::ActorCreated,
        TraceEventType::ActorFailed,
        TraceEventType::DataFlow { 
            to_actor: "analytics_engine".to_string(),
            to_port: "input".to_string(),
        },
    ]),
    status_filter: Some(vec![ExecutionStatus::Failed]),
};
```

## Best Practices

### Event Granularity

- **High-Frequency Operations**: Use sampling for message passing in high-throughput scenarios
- **Critical Operations**: Always trace actor lifecycle events and failures
- **Debug Information**: Use custom attributes for debugging context

### Performance Considerations

```rust
// Efficient event creation
let event = TraceEvent::data_flow(
    source_actor, source_port,
    dest_actor, dest_port,
    message_type, message_size
);

// Batch similar events
if batch.len() >= batch_size {
    tracing.record_batch(batch).await?;
    batch.clear();
}
```

### Security & Privacy

```rust
// Filter sensitive data
let safe_attributes = attributes.into_iter()
    .filter(|(key, _)| !SENSITIVE_FIELDS.contains(key))
    .collect();

// Hash sensitive identifiers
let user_hash = format!("user_{}", hash(&user_id));
attributes.insert("user_hash", json!(user_hash));
```

## Query Patterns

### Common Queries

```rust
// Find all failed actors in last hour
let query = TraceQuery {
    time_range: Some((Utc::now() - Duration::hours(1), Utc::now())),
    status: Some(ExecutionStatus::Failed),
    ..Default::default()
};

// Trace data flow for specific message type
let query = TraceQuery {
    actor_filter: Some(".*processor.*".to_string()), // Regex
    event_types: Some(vec![TraceEventType::DataFlow { 
        to_actor: "*".to_string(), 
        to_port: "*".to_string() 
    }]),
    ..Default::default()
};
```

### Performance Analysis

```rust
// Find slowest operations
SELECT actor_id, AVG(execution_time_ns) as avg_time_ns
FROM trace_events 
WHERE event_type = 'ActorCompleted'
  AND timestamp > NOW() - INTERVAL '1 hour'
GROUP BY actor_id
ORDER BY avg_time_ns DESC;
```

This comprehensive event reference provides the foundation for effective observability in Reflow actor networks. Each event type serves specific monitoring and debugging use cases, enabling deep insights into system behavior and performance.
