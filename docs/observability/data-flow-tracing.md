# Data Flow Tracing

Data Flow Tracing is a core component of Reflow's observability framework, providing automatic and comprehensive tracking of data movement between actors in your network. This feature gives you unprecedented visibility into how information flows through your system.

## Overview

Traditional actor monitoring focuses on individual actor behavior - creation, completion, and failures. Data Flow Tracing extends this by capturing the **connections** between actors, providing insights into:

- **Message Routing**: How messages travel through your actor network
- **Data Lineage**: Complete paths of data transformation
- **Performance Bottlenecks**: Where data flow slows down or gets congested
- **System Dependencies**: Which actors depend on which data sources

## How Data Flow Tracing Works

### Automatic Capture

Data Flow Tracing operates at the **connector level**, intercepting messages as they flow between actors:

```rust
// Automatic tracing at the connector level (simplified). The message itself is
// passed as content; the integration computes the snapshot (checksum, size, and
// — if capture_content is on — the bytes) per the configured capture knobs.
if let Some(tracing) = &network.tracing_integration {
    tracing.trace_data_flow(
        from_actor_id, from_port,
        to_actor_id, to_port,
        msg.type_name(),                       // message type label
        &msg,                                   // content (Message: Serialize)
        reflow_tracing_protocol::PerformanceMetrics::default(),
    ).await?;
}
```

This approach provides several advantages:

- **Zero Configuration**: Works immediately with existing actor networks
- **Complete Coverage**: Captures all message flows without missing any
- **Accurate Timing**: Records actual transmission times
- **Minimal Overhead**: Efficient implementation with batching

### Event Structure

Data Flow events contain rich metadata about the message transfer:

```rust
pub struct DataFlowEvent {
    // Standard event fields
    event_id: EventId,
    timestamp: DateTime<Utc>,
    event_type: TraceEventType::DataFlow {
        to_actor: String,    // Destination actor
        to_port: String,     // Destination port
    },
    actor_id: String,        // Source actor (from_actor)
    
    // Data flow specific information
    data: TraceEventData {
        port: Some("output".to_string()),  // Source port
        message: Some(MessageSnapshot {
            message_type: "SensorReading".to_string(),
            size_bytes: 256,
            checksum: "sha256:abc123...",
            serialized_data: vec![], // Optional data capture
        }),
        performance_metrics: PerformanceMetrics {
            execution_time_ns: 1_500_000,  // 1.5ms transfer time
            queue_depth: 3,                // Destination queue depth
            throughput_msgs_per_sec: 1000.0,
            memory_usage_bytes: 512,       // Memory for message processing
            cpu_usage_percent: 2.5,
        },
        custom_attributes: HashMap::from([
            ("source_actor", json!("sensor_reader")),
            ("source_port", json!("data")),
            ("destination_actor", json!("data_processor")),
            ("destination_port", json!("input")),
            ("message_id", json!("msg_12345")),
            ("protocol", json!("memory_channel")),
            ("compression", json!("none")),
        ]),
        ..Default::default()
    },
}
```

## Use Cases

### 1. Data Lineage Tracking

Track how data flows and transforms through your entire pipeline:

```mermaid
graph LR
    A[Sensor Reader] -->|SensorReading| B[Data Validator]
    B -->|ValidatedReading| C[Data Transformer]
    C -->|ProcessedData| D[Analytics Engine]
    D -->|Insights| E[Dashboard]
    
    style A fill:#e1f5fe
    style E fill:#f3e5f5
```

Query for complete data lineage. `TraceQuery` has these fields: `flow_id`,
`execution_id`, `time_range`, `status`, `actor_filter`, `limit`, `offset`.
There is no `event_types`/`custom_filter` field — filter the returned events
client-side (e.g. by `event_type` or `data.message.checksum`):

```rust
use reflow_tracing_protocol::{TraceEventType, TraceQuery};

let query = TraceQuery {
    flow_id: None,
    execution_id: None,
    time_range: None,
    status: None,
    actor_filter: Some("data_processor".to_string()),
    limit: Some(200),
    offset: None,
};

let traces = tracing_client.query_traces(query).await?;
// Narrow to data-flow events for a specific payload by its content checksum:
let lineage: Vec<_> = traces.iter()
    .flat_map(|t| &t.events)
    .filter(|e| matches!(e.event_type, TraceEventType::DataFlow { .. }))
    .filter(|e| e.data.message.as_ref()
        .map(|m| m.checksum == "sha256:9f86d081…")
        .unwrap_or(false))
    .collect();
```

### 2. Performance Analysis

Identify bottlenecks. Query by time range, then filter on the (optional)
performance metrics client-side. Note `execution_time_ns` is `Option<u64>`
(`None` = unmeasured), and the heavy fields require `enable_perf_sampling`:

```rust
let recent = TraceQuery {
    flow_id: None,
    execution_id: None,
    time_range: Some((Utc::now() - Duration::hours(1), Utc::now())),
    status: None,
    actor_filter: None,
    limit: Some(500),
    offset: None,
};
let traces = tracing_client.query_traces(recent).await?;
let slow: Vec<_> = traces.iter()
    .flat_map(|t| &t.events)
    .filter(|e| matches!(e.event_type, TraceEventType::DataFlow { .. }))
    .filter(|e| e.data.performance_metrics.execution_time_ns
        .map(|ns| ns > 10_000_000)   // > 10ms; ignores unmeasured (None)
        .unwrap_or(false))
    .collect();
```

### 3. System Dependency Mapping

Understand which actors depend on which data sources:

```sql
-- Find most active data flows
SELECT 
    source_actor,
    destination_actor,
    COUNT(*) as message_count,
    AVG(execution_time_ns) as avg_transfer_time,
    SUM(size_bytes) as total_bytes
FROM data_flow_events 
WHERE timestamp > NOW() - INTERVAL '1 hour'
GROUP BY source_actor, destination_actor
ORDER BY message_count DESC;
```

### 4. Real-time Monitoring

Monitor data flow in real-time for operational awareness:

```rust
// Subscribe to data flow events for specific actors
let filters = SubscriptionFilters {
    actor_ids: Some(vec!["critical_processor".to_string()]),
    event_types: Some(vec![TraceEventType::DataFlow { 
        to_actor: "*".to_string(), 
        to_port: "*".to_string() 
    }]),
    ..Default::default()
};

tracing_client.subscribe(filters).await?;
```

## Configuration

### Enabling Data Flow Tracing

Data Flow Tracing is enabled automatically when you enable the observability framework:

```rust
let tracing_config = TracingConfig {
    server_url: "ws://localhost:8080".to_string(),
    enabled: true,                    // Enables all tracing including data flow
    batch_size: 50,                  // Batch size for data flow events
    batch_timeout: Duration::from_millis(1000),
    enable_compression: true,         // Recommended for data flow events
    ..Default::default()
};
```

### Selective Tracing

> The built-in controls are the `capture_checksum` / `capture_content` config
> toggles (cheap identity always on; heavy content opt-in). The
> `SelectiveConnector` / `DataFlowSampler` / `should_trace_message` code below is
> **illustrative user-authored** sampling — there is no such built-in type. Note
> the real `trace_data_flow` signature takes `(…, message_type, &content, metrics)`.

For high-throughput systems, you might want to selectively trace certain data flows:

```rust
// Custom connector with selective tracing
impl SelectiveConnector {
    pub async fn send_message(&self, message: Message) -> Result<()> {
        self.channel.send(message.clone()).await?;
        
        // Only trace certain message types or conditions
        if should_trace_message(&message) {
            if let Some(tracing) = global_tracing() {
                tracing.trace_data_flow(
                    &self.from_actor, &self.from_port,
                    &self.to_actor, &self.to_port,
                    message.type_name(), message.size_bytes()
                ).await?;
            }
        }
        
        Ok(())
    }
}

fn should_trace_message(message: &Message) -> bool {
    // Trace based on message type, size, or other criteria
    match message.type_name() {
        "CriticalAlert" => true,        // Always trace alerts
        "DebugInfo" => false,           // Never trace debug info
        "DataUpdate" if message.size_bytes() > 1024 => true, // Large updates only
        _ => rand::random::<f64>() < 0.1, // Sample 10% of other messages
    }
}
```

### Sampling Configuration

For extremely high-throughput scenarios, implement sampling:

```rust
pub struct DataFlowSampler {
    sample_rate: f64,      // 0.0 to 1.0
    always_trace: Vec<String>, // Actor names to always trace
    never_trace: Vec<String>,  // Actor names to never trace
}

impl DataFlowSampler {
    pub fn should_trace(&self, from_actor: &str, to_actor: &str) -> bool {
        if self.never_trace.contains(&from_actor.to_string()) ||
           self.never_trace.contains(&to_actor.to_string()) {
            return false;
        }
        
        if self.always_trace.contains(&from_actor.to_string()) ||
           self.always_trace.contains(&to_actor.to_string()) {
            return true;
        }
        
        rand::random::<f64>() < self.sample_rate
    }
}
```

## Content fidelity (checksums & capture)

Every data-flow event carries a `MessageSnapshot`. Fidelity is governed by two
config toggles, not hand-rolled helpers:

```rust
let tracing_config = TracingConfig {
    server_url: "ws://localhost:8080".to_string(),
    enabled: true,
    capture_checksum: true,   // default ON  — cheap content digest
    capture_content: false,   // default OFF — retain raw bytes (heavy/sensitive)
    ..TracingConfig::default()
};
```

### Checksum (always-on, cheap)

With `capture_checksum` on (the default), each snapshot gets a content-only
`"sha256:<64 lowercase hex>"` digest over a canonical form of the message —
identical across processes, hosts, CPU architectures, and SDK languages. Use it
for content identity, dedup, and integrity:

```rust
if let Some(msg) = &event.data.message {
    println!("type={} size={}B checksum={}", msg.message_type, msg.size_bytes, msg.checksum);
}
```

`size_bytes` is the pre-compression content size (the same bytes the checksum
covers). The digest is computed over the *decompressed* content, so toggling
compression never changes it.

### Content capture (opt-in, heavy)

`capture_content` additionally retains the message bytes in
`serialized_data` (self-describing via `content_codec` + `content_format_version`,
with `stored_bytes` for the stored footprint). Invariant:
`checksum == sha256(canonical(decompress(serialized_data)))`.

⚠️ **Security**: captured content may contain sensitive payloads. Keep
`capture_content` off unless you need full replay, and scope it narrowly.

### Causality fields

Each event has a `causality` block (`parent_event_id`, `root_cause_event_id`,
`dependency_chain`, `span_id`). Across process boundaries the inbound hop is
linked to the sending span (see the distributed-tracing section in the
[overview](overview.md)).
Fine-grained per-message causal chaining within a process is a documented
follow-up — the fields exist but are not yet auto-populated for in-process hops.

## Performance Considerations

### Overhead Analysis

Data Flow Tracing introduces minimal overhead:

- **Memory**: ~200 bytes per event
- **CPU**: ~0.1ms per event (including serialization)
- **Network**: Batched transmission reduces network calls
- **Storage**: ~1KB per event when stored

### Optimization Strategies

1. **Batching**: Use larger batch sizes for high-throughput scenarios
2. **Compression**: Enable compression for network transmission
3. **Sampling**: Sample events rather than capturing every one
4. **Filtering**: Use selective tracing based on criticality
5. **Async Processing**: All tracing operations are non-blocking

### Monitoring Performance Impact

The server tracks basic counters (connections, messages, traces stored/queried),
queryable via the legacy `TraceMessage::GetMetrics` request. There is no
`global_tracing().get_performance_metrics()` API. The cheap path
(`capture_checksum` on, `capture_content` off, `enable_perf_sampling` off) keeps
per-event overhead to a checksum; turn `capture_content` / `enable_perf_sampling`
on only when you need the extra data.

## Visualization and Analysis

### Data Flow Diagrams

There is no built-in `DataFlowGraph`/renderer. Query the data-flow events and
build a graph from them in your tool of choice — each `DataFlow` event gives you
the `actor_id` (source), `event_type.DataFlow { to_actor, to_port }`
(destination), and `data.message` (type/size/checksum):

```rust
let traces = tracing_client.query_traces(TraceQuery {
    flow_id: None, execution_id: None,
    time_range: Some((Utc::now() - Duration::hours(1), Utc::now())),
    status: None, actor_filter: None, limit: Some(1000), offset: None,
}).await?;

for e in traces.iter().flat_map(|t| &t.events) {
    if let TraceEventType::DataFlow { to_actor, to_port } = &e.event_type {
        // edge: e.actor_id -> to_actor (port to_port), size e.data.message…
    }
}
```

### Real-time Dashboard

Build real-time monitoring dashboards:

```javascript
// WebSocket connection for real-time data flow monitoring
const ws = new WebSocket('ws://tracing-server:8080');

ws.onmessage = (event) => {
    const traceEvent = JSON.parse(event.data);
    if (traceEvent.event_type.DataFlow) {
        updateDataFlowVisualization(traceEvent);
    }
};
```

## Troubleshooting

### Common Issues

**No Data Flow Events Appearing**:
- Verify tracing is enabled: `enabled: true`
- Check that actors are connected via standard connectors
- Ensure global tracing is initialized before network operations

**Too Many Events**:
- Implement sampling: reduce `sample_rate`
- Use selective tracing for specific actors only
- Increase `batch_size` to reduce network overhead

**Performance Impact**:
- Enable compression: `enable_compression: true`
- Use PostgreSQL backend for better concurrent performance
- Consider async event processing

### Debugging Data Flow Issues

Use data flow tracing to debug connectivity and performance issues:

```rust
// Debug missing data flows
let missing_flows = TraceQuery {
    actor_filter: Some("source_actor".to_string()),
    event_types: Some(vec![TraceEventType::MessageSent]),
    time_range: Some((start_time, end_time)),
    ..Default::default()
};

let sent_messages = tracing_client.query_traces(missing_flows).await?;

// Check if corresponding DataFlow events exist
for sent_event in sent_messages {
    let corresponding_flow = find_data_flow_for_message(&sent_event).await?;
    if corresponding_flow.is_none() {
        println!("Missing data flow for message: {:?}", sent_event);
    }
}
```

## Best Practices

1. **Start Simple**: Begin with default settings and tune based on your needs
2. **Monitor Overhead**: Keep an eye on the performance impact of tracing
3. **Use Sampling**: For high-throughput systems, sample rather than trace everything
4. **Secure Sensitive Data**: Never trace sensitive message content
5. **Regular Cleanup**: Set up automatic cleanup of old trace data
6. **Correlate Events**: Use causality tracking to link related events
7. **Custom Metadata**: Add domain-specific metadata for better insights

Data Flow Tracing provides unprecedented visibility into your actor network's communication patterns. Use it to understand, debug, and optimize your distributed systems with confidence.
