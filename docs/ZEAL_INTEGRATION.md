# Zeal ↔ Reflow Integration

How Reflow connects to Zeal as a runtime provider.

## Connection Lifecycle

```
Reflow starts → connects to Zeal via ZIP SDK
    │
    ├── 1. Register categories (stream, media, 3d)
    ├── 2. Register templates (actors + properties + display components)
    ├── 3. Register webhook (tells Zeal where to reach us)
    ├── 4. Open WebSocket for real-time events
    └── 5. Listen for execution commands
```

## Workflow Lifecycle

### 1. Orchestration (Zeal)

User builds a graph in Zeal's canvas using registered templates.
Each node maps to a Reflow actor via `template_id`.

### 2. Publish (Zeal → Reflow)

Zeal pushes the workflow graph to Reflow:

```
POST /workflows/{workflow_id}/publish
Body: { "workflow": ZealWorkflow }

Response: {
  "workflow_id": "my-flow",
  "status": "published",
  "webhook": "/webhook/my-flow"
}
```

Reflow stores the graph in `ExecutionEngine::workflow_graphs` for later triggering.

### 3. Execute (trigger)

Three ways to trigger execution:

**a) Zeal WebSocket command** (preferred)
```
User clicks Execute in Zeal
  → Zeal sends execution command over ZIP WebSocket
  → Reflow's zip_session receives it
  → engine.start_zeal_execution(workflow, input)
```

**b) Webhook (REST)**
```
POST /webhook/{workflow_id}
Body: { "data": "..." }
Headers: Content-Type, Authorization, etc.

  → engine.start_webhook_execution(workflow_id, request_data)
  → Finds stored graph, injects request into ServerRequestActor config
  → Starts network execution
```

**c) Direct execution (REST)**
```
POST /zeal/workflows
Body: { "workflow": ZealWorkflow, "input": {...} }

  → engine.start_zeal_execution(workflow, input)
  → Converts Zeal format → Reflow graph, executes
```

### 4. Execution Events (Reflow → Zeal)

During execution, events flow back to Zeal via WebSocket:

```
Actor starts    → node.executing { nodeId, inputConnections }
Actor completes → node.completed { nodeId, outputConnections, duration }
Actor fails     → node.failed { nodeId, error }
Stream opened   → stream.opened { nodeId, streamId, contentType }
Stream frame    → binary WebSocket frame [type:1][id:8][payload]
Stream closed   → stream.closed { nodeId, totalBytes }
Workflow done   → execution.completed { duration, nodesExecuted }
```

## Template Registration

### Categories

```rust
// Registered at connect time via register_categories()
stream     → plumbing
media      → images, audio, filters, dynamics, analysis, spectral, procedural, export
3d         → sdf, operations, transforms, export
tools-utilities → math, triggers, files
```

### Template Structure

Each template is registered with:
- `id`: template ID (e.g. `tpl_sdf_sphere`)
- `type`: node type for Zeal's renderer lookup
- `title`/`subtitle`: display names
- `category`/`subcategory`: palette grouping
- `ports`: typed input/output connections
- `properties`: editable config fields with types, defaults, min/max
- `display`: optional inline Web Component for custom node UI

### Property Flow

```
Zeal template:
  properties: { radius: { type: "number", defaultValue: 1.0, min: 0.01 } }

User sets in property panel:
  property_values: { radius: 2.5 }

Zeal → Reflow converter flattens:
  node_metadata: { radius: 2.5 }  // user value overrides default

Actor reads:
  let radius = config.get("radius").and_then(|v| v.as_f64()).unwrap_or(1.0);
```

### Input Nodes → IIPs

Zeal input widgets (`tpl_text_input`, `tpl_number_input`, `tpl_range_input`)
are NOT actors. The converter transforms them into Initial Information Packets:

```
[Number Input: 42] ──→ [MathAdd.a]

Becomes:
  graph.add_initial(42, "math_add_node", "a")
```

## Display Components

Reflow provides inline Web Components for custom node rendering:

```
template.display = {
  element: "reflow-spectrum",
  source: "class ReflowSpectrum extends ReflowUI.ReflowComponent { ... }",
  shadow: true,
  observedProps: ["fftSize"],
}
```

Components use Zeal's bridge API:
- `this.zeal.getProperty(name)` / `setProperty(name, value)` — read/write config
- `this.zeal.onStreamFrame(callback)` — receive binary stream data
- `this.zeal.onPropertyChange(callback)` — react to side panel edits

Shared library `reflow-ui.js` provides:
- `ReflowComponent` base class with auto-cleanup
- Theme tokens, Tabler icons, canvas helpers
- Meter widgets, format utilities

## ServerRequest/Response Pattern

`ServerRequestActor` is a source actor (no inports). When a webhook
triggers, the server injects the HTTP request data into the actor's
config before execution:

```
Webhook POST /webhook/my-flow { body: {...}, headers: {...} }
  │
  └→ engine modifies graph: ServerRequestActor.metadata += request_data
     │
     └→ Actor reads config.get("body"), config.get("headers"), etc.
        │
        └→ Outputs on ports: body, headers, params, method, url
```

`ServerResponseActor` collects the processed response and outputs
it as a structured response object for the server to return.

## Binary Stream Protocol

For stream display (audio/video/image preview), binary frames go
directly over the ZIP WebSocket:

```
[1 byte: frame_type] [8 bytes: stream_id LE] [payload]

0x01 = Begin  → JSON metadata { content_type, size_hint }
0x02 = Data   → raw bytes (image row, audio chunk)
0x03 = End    → empty
0x04 = Error  → UTF-8 message
```

The observer tap in `EventBridge` attaches to streams and forwards
frames to Zeal without blocking the actor pipeline.

## File Reference

| File | Purpose |
|------|---------|
| `crates/reflow_server/src/zip_session.rs` | ZIP connection, template/category/webhook registration |
| `crates/reflow_server/src/rest_api.rs` | REST endpoints: publish, webhook, execute |
| `crates/reflow_server/src/engine.rs` | Execution engine, workflow storage, graph building |
| `crates/reflow_server/src/event_bridge.rs` | Engine events → ZIP WebSocket + trace collector |
| `crates/reflow_server/src/template_metadata.rs` | Rich template definitions for all actors |
| `crates/reflow_server/src/zeal_converter.rs` | Zeal workflow → Reflow graph conversion |
| `display_components/` | Web Component JS sources for node rendering |
| `display_components/reflow-ui.js` | Shared component library (theme, icons, base class) |
