# Tutorial 05 — Spring Boot REST + per-request Reflow network

Runnable code for [Real-world Reflow 05: Parallel data enrichment
behind a Spring Boot endpoint](../../../../docs/tutorials/real-world/05-spring-enrich.md).

A POST to `/enrich` spins up a fresh Reflow network shaped like a
fan-out / fan-in:

```
Splitter ─┬─► Inventory ─┐
          ├─► Price      ├─► Merger ─► JSON response
          └─► Reviews ───┘
```

The `Merger` sets `awaitAllInports = true` so its `run` fires once
when every parallel branch has produced its packet — Reflow's
equivalent of `CompletableFuture.allOf(...).join()`.

## Run

```sh
cd sdk/jvm/examples/tutorial-05-spring-enrich
gradle bootRun
```

Then in another terminal:

```sh
curl -s localhost:8080/enrich \
  -H 'content-type: application/json' \
  -d '{"sku":"WIDGET-42"}' | jq
```

Sample response:

```json
{
  "inventory": {"sku": "WIDGET-42", "stock": 63},
  "price":     {"amount": 14.49, "currency": "USD", "sku": "WIDGET-42"},
  "reviews":   {"avg": 3.5, "count": 27, "sku": "WIDGET-42"}
}
```

Total wall-clock time is dominated by the slowest branch (~220 ms),
not the sum of all three (~550 ms) — the fan-out is real. (Inner
keys come back alphabetized because Reflow round-trips the payload
through `serde_json::Value` between actors.)

## Test

```sh
gradle test
```

`EnrichTest` boots the full Spring context, posts a request, and
asserts the merged response shape. Per-request network lifecycle
is exercised end-to-end.

## How it works

The Reflow JVM SDK auto-loads its native library from the published
JAR — no manual `cargo build`, no path overrides. Just declare the
dependency:

```kotlin
implementation("ai.offbit:reflow:0.2.7")
```

`Network` is `AutoCloseable`, so the controller wraps each request
in `try (var net = new Network()) { ... }`. When `done.get()`
returns or the request times out, the network shuts down cleanly.
