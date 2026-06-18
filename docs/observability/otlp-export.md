# OpenTelemetry (OTLP) Export — Monoscope & friends

The `reflow_tracing` collector can **export finalized traces to any OpenTelemetry
backend** over OTLP/HTTP — [Monoscope](https://github.com/monoscope-tech/monoscope),
Jaeger, Grafana Tempo, Honeycomb, Uptrace, or a vanilla OpenTelemetry Collector.
This is the bridge from reflow's own trace store to an operator-facing monitoring
dashboard.

It's complementary, not either/or:

- the **bespoke collector** stays the reflow-native system of record (FlowTrace
  replay, content checksums, causality, the distributed unified-trace model);
- the **OTLP export** feeds a dashboard layer for live waterfalls, service maps,
  alerting, and correlation with the surrounding app's logs/metrics.

Because the integration is the **OTLP wire protocol** (not linking any backend's
code), it works with every OTel system and carries no library/license coupling.

## Enable it

Build the server with the `otlp` feature and add an `[otlp]` block:

```bash
cargo build -p reflow_tracing --features otlp
```

```toml
[otlp]
enabled = true
# Base OTLP/HTTP endpoint; "/v1/traces" is appended automatically.
# Monoscope / an OTel Collector listens for OTLP/HTTP on :4318.
endpoint = "http://localhost:4318"
service_name = "reflow"
```

Each trace is exported once, when it **finalizes** (on `EndTrace` or client
disconnect), as a best-effort fire-and-forget POST — export never blocks the
collector or the data plane. A failed export is logged at `debug` and dropped.

## How traces map to OpenTelemetry

reflow's trace model is already span-shaped, so the mapping is direct:

| reflow | OpenTelemetry span(s) |
|---|---|
| `FlowTrace.trace_id` (128-bit UUID) | the OTel **trace id** (16 bytes), used directly |
| the flow as a whole | a synthetic **root span** `flow:{flow_id}` over `start_time`..`end_time` |
| each `TraceEvent` | a **child span**, parented to `causality.parent_event_id` (else the root) |
| `DataFlow { to_actor, to_port }` | span `dataflow:{from}->{to}` + `reflow.to_actor` / `reflow.to_port` attrs |
| `PerformanceMetrics.execution_time_ns` | span **duration** (`end = start + ns`) |
| `MessageSnapshot.checksum` | attribute `reflow.message.checksum` |
| `ActorFailed` (+ `error`) | span **status = ERROR** with the message |
| cross-process `TraceContext` (shared collector) | one **distributed trace** in the dashboard |

Per the OTLP/JSON encoding, trace/span ids are hex strings and timestamps are
decimal unix-nanos strings; the payload is a standard
`ExportTraceServiceRequest` (`resourceSpans → scopeSpans → spans`).

## Monoscope specifically

Monoscope is OpenTelemetry-native (logs + traces + metrics, 750+ integrations,
self-hostable via Docker Compose). Point `endpoint` at its OTLP/HTTP listener and
reflow flows show up alongside everything else it ingests — including the
**cross-process flows** stitched by the [shared-collector model](overview.md),
which render as single distributed traces.

> Don't write traces into Monoscope's internal store directly (its TimeFusion DB
> is Postgres-compatible, so it's tempting): OTLP is the supported ingestion
> path and keeps you decoupled from its schema.
