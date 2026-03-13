# Zeal IDE Integration

Reflow connects to Zeal IDE via the ZIP (Zeal Integration Protocol) for template registration, real-time event streaming, and trace session submission. This connection is established by the `ZipSession` module in `reflow_server`.

## Architecture

```mermaid
graph LR
    subgraph "Reflow Node"
        ENG[ExecutionEngine]
        EB[EventBridge]
        TC[TraceCollector]
        ZIP[ZipSession]
    end

    subgraph "Zeal IDE"
        TR[Template Registry]
        WS[/ws/zip WebSocket]
        TA[TracesAPI]
    end

    ENG -->|EngineEvent| EB
    EB -->|forward| TC
    EB -->|forward| ZIP

    ZIP -->|register templates| TR
    ZIP -->|ZIP events| WS
    TC -->|trace sessions| TA
```

## Configuration

The ZIP session is activated when `zeal_url` is set in `ServerConfig`:

```rust
let config = ServerConfig {
    zeal_url: Some("http://localhost:3000".to_string()),
    namespace: "reflow".to_string(),
    node_id: "reflow-abc12345".to_string(),
    // ...
};
```

Or via environment/config file:

```json
{
  "zeal_url": "http://localhost:3000",
  "namespace": "reflow",
  "node_id": "reflow-node-1"
}
```

When `zeal_url` is `None`, the server runs in headless mode with no Zeal connection.

## ZIP Session Lifecycle

### 1. Template Registration

On startup, the session registers all available actor templates with Zeal:

```
POST /api/templates/register
{
  "namespace": "reflow",
  "templates": [
    {
      "id": "tpl_http_request",
      "title": "http request",
      "category": "reflow",
      "icon": "cpu",
      "runtime": { "executor": "reflow", "version": "0.1.0" }
    },
    // ... 10 native templates + 6,697 API actor templates
  ]
}
```

Native actors are registered with basic metadata. API actors include additional fields:
- `subcategory` — service name (e.g., "Slack")
- `icon` — brand icon
- `required_env_vars` — authentication keys needed
- `ports` — typed input/output port declarations

### 2. WebSocket Connection

After registration, the session opens a WebSocket to Zeal's `/ws/zip` endpoint:

```
ws://localhost:3000/ws/zip
```

The URL is derived from the HTTP URL by replacing `http(s)://` with `ws(s)://` and appending the ZIP WebSocket path.

### 3. Event Streaming

During workflow execution, the `EventBridge` forwards `EngineEvent`s to the `ZipSession`, which translates them into `ZipExecutionEvent`s and sends them as JSON text frames:

| EngineEventType | ZipExecutionEvent |
|-----------------|-------------------|
| `Started` | `ExecutionStarted` |
| `NodeExecuting` | `NodeExecuting` |
| `ActorCompleted` | `NodeCompleted` (with `duration`, `output_size`) |
| `ActorFailed` | `NodeFailed` (with error details) |
| `Completed` | `ExecutionCompleted` (with summary stats) |
| `Failed` | `ExecutionFailed` (with error) |

Events like `MessageSent` and `NetworkIdle` have no direct ZIP mapping and are silently dropped.

### 4. Shutdown

The session shuts down gracefully when `shutdown()` is called, which notifies the event loop via a `tokio::sync::Notify`.

## TraceCollector

In addition to real-time WebSocket events, the `TraceCollector` submits detailed per-node trace data to Zeal's TracesAPI over HTTP:

### Session Lifecycle

1. **Begin** — `POST /api/traces/sessions` creates a trace session for the execution
2. **Submit Events** — `POST /api/traces/sessions/{id}/events` submits batched `TraceEvent`s (batch size: 50)
3. **Complete** — `POST /api/traces/sessions/{id}/complete` finalizes with a `SessionSummary`

### Trace Events

Each `EngineEvent` is translated into a `TraceEvent` with:

- `timestamp` — event time
- `node_id` — actor/node identifier
- `event_type` — `Input`, `Output`, or `Error`
- `data` — `TraceData` with size, data type, and optional preview
- `duration` — processing time (for completed nodes)
- `error` — error details (for failed nodes)

### Session Summary

On completion, a summary is submitted:

```rust
SessionSummary {
    total_nodes: 10,
    successful_nodes: 8,
    failed_nodes: 2,
    total_duration: 1500,        // ms
    total_data_processed: 45000, // bytes
}
```

## EventBridge

The `EventBridge` is the glue between the execution engine and the observability consumers. One bridge task is spawned per execution:

```rust
// In REST API handler after starting execution
if let Some(bridge) = &state.event_bridge {
    bridge.attach(workflow_id, execution_id, event_rx);
}
```

The bridge:
1. Begins a trace session via `TraceCollector`
2. Drains the engine's `flume::Receiver<EngineEvent>` channel
3. Forwards each event to both `TraceCollector` (for HTTP traces) and `ZipSession` (for WebSocket)
4. Tracks terminal state (success/failure)
5. Completes the trace session when the channel closes

## Next Steps

- [REST API](./rest-api.md) - HTTP and WebSocket API for headless execution
- [Observability Architecture](../observability/architecture.md) - Detailed event pipeline
- [Event Types](../observability/event-types.md) - EngineEvent and ZIP event reference
