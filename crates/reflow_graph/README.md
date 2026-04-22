# reflow_graph

Graph data structures for Reflow — nodes, edges, initial packets, exposed ports, subgraph boundaries, import/export, and graph analysis.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt)** which re-exports this crate as `reflow_rt::graph`. Depend directly on `reflow_graph` only for graph tooling, editors, or serializers that do not need the runtime executor.

## What it provides

- `Graph` — in-memory graph model with nodes, connections, and IIPs.
- `GraphExport` — serializable snapshot that editors and tools read and write.
- Graph history, validation, and lightweight analysis helpers.

## Quick glance

```rust
use reflow_graph::{Graph, types::GraphExport};

// Author a graph programmatically.
let mut graph = Graph::new("example", false, None);
graph.add_node("tap", "tpl_passthrough", None);

// Or load one that a tool produced.
let json = std::fs::read_to_string("flow.json")?;
let export: GraphExport = serde_json::from_str(&json)?;
let loaded = Graph::load(export, None);
```

Executing a graph requires a `Network` from [`reflow_network`](https://docs.rs/reflow_network) and registered actors from [`reflow_components`](https://docs.rs/reflow_components). Start at `reflow_rt` for that.

## License

MIT OR Apache-2.0.
