# REST API

The Reflow server exposes an HTTP and WebSocket API for headless workflow execution. This is the primary interface for clients that don't use the Zeal IDE.

Built with Axum, the API provides workflow execution, status monitoring, Zeal format conversion, and real-time WebSocket streaming.

## Base URL

```
http://{bind_address}:{port}
```

Default: `http://0.0.0.0:8080`

## Endpoints

### Health Check

```
GET /health
```

Returns server health status.

**Response:**
```json
{
  "success": true,
  "data": "Server is healthy",
  "timestamp": 1710300000000
}
```

### Start Workflow

```
POST /workflows
```

Starts a workflow execution from a Reflow graph JSON.

**Request Body:**
```json
{
  "graph_json": {
    "processes": {},
    "connections": [],
    "inports": {},
    "outports": {},
    "groups": [],
    "properties": {}
  },
  "input": {},
  "metadata": {
    "execution_id": "exec-001",
    "workflow_id": "workflow-001",
    "source": "api",
    "webhook_url": null,
    "enable_tracing": true
  }
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "execution_id": "exec-001",
    "status": "started",
    "message": "Workflow started in background"
  },
  "timestamp": 1710300000000
}
```

When an `EventBridge` is configured (Zeal connection active), execution events are automatically forwarded to TraceCollector and ZipSession.

### Get Execution Status

```
GET /workflows/{execution_id}
```

Returns the current state of an execution.

**Response:**
```json
{
  "success": true,
  "data": {
    "id": "exec-001",
    "status": "completed",
    "result": { ... }
  },
  "timestamp": 1710300000000
}
```

**Status values:** `queued`, `running`, `completed`, `failed`, `cancelled`

### Cancel Workflow

```
POST /workflows/{execution_id}/cancel
```

Cancels a running workflow execution.

**Response:**
```json
{
  "success": true,
  "data": {
    "execution_id": "exec-001",
    "status": "cancelled"
  },
  "timestamp": 1710300000000
}
```

### Execute Zeal Workflow

```
POST /zeal/workflows
```

Accepts a Zeal-format workflow, converts it to Reflow graph format, and executes it.

**Request Body:**
```json
{
  "workflow": {
    "id": "wf-001",
    "name": "My Workflow",
    "graphs": [
      {
        "nodes": [...],
        "connections": [...]
      }
    ]
  },
  "input": {}
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "execution_id": "auto-generated-id",
    "status": "started",
    "message": "Zeal workflow converted and started"
  },
  "timestamp": 1710300000000
}
```

### Convert Zeal Workflow

```
POST /zeal/convert
```

Converts a Zeal workflow to Reflow graph format without executing it. Useful for inspection and debugging.

**Request Body:** A `ZealWorkflow` JSON object.

**Response:**
```json
{
  "success": true,
  "data": {
    "reflow_graph": { ... },
    "required_actors": ["tpl_http_request", "tpl_if_branch"],
    "conversion_metadata": {
      "source_workflow_id": "wf-001",
      "source_workflow_name": "My Workflow",
      "node_count": 5,
      "connection_count": 4
    }
  },
  "timestamp": 1710300000000
}
```

## WebSocket API

```
GET /ws
```

Upgrades to a WebSocket connection for real-time workflow interaction.

### Message Format

All messages are JSON with a `type` field:

```json
{ "type": "message_type", ... }
```

### Start Workflow (WebSocket)

```json
{
  "type": "start_workflow",
  "data": {
    "graph_json": { ... },
    "input": { ... },
    "metadata": {
      "execution_id": "exec-001",
      "workflow_id": "workflow-001",
      "source": "websocket"
    }
  }
}
```

**Response:**
```json
{
  "type": "workflow_started",
  "success": true,
  "execution_id": "exec-001"
}
```

### Subscribe to Events

```json
{
  "type": "subscribe_workflow",
  "execution_id": "exec-001"
}
```

**Acknowledgement:**
```json
{
  "type": "subscription_ack",
  "success": true,
  "execution_id": "exec-001"
}
```

**Event stream (per network event):**
```json
{
  "type": "network_event",
  "execution_id": "exec-001",
  "event": { ... },
  "timestamp": 1710300000000
}
```

### Cancel Workflow (WebSocket)

```json
{
  "type": "cancel_workflow",
  "execution_id": "exec-001"
}
```

**Response:**
```json
{
  "type": "workflow_cancelled",
  "success": true,
  "execution_id": "exec-001"
}
```

## Server Configuration

```rust
pub struct ServerConfig {
    pub port: u16,                              // default: 8080
    pub bind_address: String,                   // default: "0.0.0.0"
    pub max_connections: usize,                 // default: 1000
    pub cors_enabled: bool,                     // default: true
    pub rate_limit_requests_per_minute: usize,  // default: 100
    pub zeal_url: Option<String>,               // Zeal IDE URL (enables ZIP)
    pub namespace: String,                      // default: "reflow"
    pub node_id: String,                        // default: "reflow-{uuid8}"
}
```

CORS is enabled with a permissive policy by default via `tower_http::cors::CorsLayer`.

## Error Responses

All error responses follow the `ApiResponse` format:

```json
{
  "success": false,
  "error": "Error description",
  "timestamp": 1710300000000
}
```

HTTP status codes:
- `400` — Bad request (invalid workflow format)
- `404` — Execution not found
- `500` — Internal server error

## Next Steps

- [Zeal IDE Integration](./zeal-ide.md) - ZIP session and template registration
- [Architecture Overview](../architecture/overview.md) - System architecture
