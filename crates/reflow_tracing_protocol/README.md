# reflow_tracing_protocol

Shared tracing-protocol types used by the Reflow runtime — the wire format for structured flow events produced by the network executor.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt)**, which pulls this crate in transitively through `reflow_network`, `reflow_actor`, and `reflow_components`. Direct use is only appropriate when implementing a custom tracing consumer, exporter, or dashboard.

## What it provides

- Event and span types emitted by the network executor when the `flowtrace` feature is enabled.
- Enums and metadata structures shared between the producer side (the running network) and consumer side (trace UIs, log exporters).

## License

MIT OR Apache-2.0.
