# Reflow SDK Roadmap

Tracks outstanding work across the four official language SDKs
(`sdk/node`, `sdk/go`, `sdk/python`, `sdk/jvm`) and the C ABI
(`crates/reflow_rt_capi`) they bind to.

Legend:
- ✅ shipped
- 🟡 partial
- ⬜ planned, not started

---

## Runtime surface parity

Every SDK mirrors the frozen C ABI one-for-one. The contract is the
design, not the implementation: if something lands here it ships across
all four SDKs.

| Surface | C ABI | Node | Go | Python | JVM |
|---------|:----:|:----:|:--:|:----:|:----:|
| Message (typed) | ✅ | ✅ | ✅ | ✅ | ✅ |
| Graph + builder | ✅ | ✅ | ✅ | ✅ | ✅ |
| Network + builder | ✅ | ✅ | ✅ | ✅ | ✅ |
| Event stream | ✅ | ✅ | ✅ | ✅ | ✅ |
| Callback actors (class pattern) | ✅ | ✅ | ✅ | ✅ | ✅ |
| Bundled template catalog | ✅ | ✅ | ✅ | ✅ | ✅ |
| Subgraph builder | ✅ | ✅ | ✅ | ✅ | ✅ |
| StreamHandle producer/consumer | ✅ | ✅ | ✅ | ✅ | ✅ |
| `NetworkConfig` JSON | ✅ | ✅ | ✅ | ✅ | ⬜ |
| Multi-graph composition | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Distributed network** | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ |
| **Workspace discovery** | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ |

---

## Distributed network API — next major piece

`reflow_network::DistributedNetwork` lets a graph span multiple
processes/hosts with remote-actor proxies, a network bridge, and a
discovery layer. Everything below lives on `DistributedNetwork`.

### Scope
- `rfl_distributed_new(config_json)` — async `new()` wrapped with
  `runtime().block_on`.
- `rfl_distributed_start` / `rfl_distributed_shutdown`.
- `rfl_distributed_register_remote_actor(actor_id, remote_network_id)`
  — creates a local proxy node that forwards to the remote peer.
- Graph/network builder proxies (`add_node`, `add_connection`,
  `add_initial`, `register_actor`) — the distributed handle stores a
  `parking_lot::RwLock<Network>`, so it duplicates the mirror methods
  on rfl_network rather than sharing storage.
- Events — the local network's existing event channel surfaces.

### Non-goals for v1
- `send_to_remote_actor` (edge case; most users let the proxy handle it).
- Discovery / gossip tuning — accept defaults baked into
  `DistributedConfig`.

### SDK-side shape
Each SDK gets a `DistributedNetwork` (Node) / `reflow.Distributed` (Go)
/ `DistributedNetwork` class (Python / JVM) that mirrors `Network`'s
class shape plus `register_remote_actor`. Tests: register a remote
proxy + verify the local graph still runs.

---

## Workspace discovery

`reflow_network::multi_graph::workspace` scans a directory tree, loads
every `*.graph.json`, analyses dependencies and interfaces, and emits
a `WorkspaceComposition`.

### Scope
- `rfl_workspace_discover(config_json) -> json` — blocking wrapper
  around `WorkspaceDiscovery::discover_workspace()`.
- Return value is the composed `WorkspaceComposition` JSON for
  downstream use (compose into a single network or inspect dependency
  graph).

### Non-goals for v1
- Live file watching / hot reload.
- Incremental dependency re-resolution.

---

## Distribution + CI

Today every SDK expects a locally built native library. Before
publishing, each needs a prebuilt-binary matrix.

| SDK | Distribution mechanism | Target triples |
|-----|------------------------|----------------|
| Node (`@offbit-ai/reflow`) | prebuilt `.node` via `@napi-rs/cli` | linux-x64, linux-arm64, darwin-x64, darwin-arm64, win32-x64 |
| Python (`offbit-reflow`) | cibuildwheel / maturin `--release` wheels | same triples, per Python version (3.9–3.13) |
| Go (`github.com/offbit-ai/reflow/sdk/go`) | cgo static-link bundle or download-on-install | same triples |
| JVM (`ai.offbit:offbit-reflow`) | fat JAR with per-triple `.so/.dylib/.dll`, extracted at `System.loadLibrary` | same triples |

CI jobs: `release.yml` with a matrix that builds each triple, uploads
artifacts per-SDK, then publishes to npm / PyPI / Maven Central (via
staging repo) / Go proxy on tag push.

---

## SDK-specific polish

- **Node**: add a typed JSON codec that handles Message.Object values
  end-to-end (right now callers either pass plain JS values through
  the Object factory or deal with JSON-shaped Messages).
- **Go**: add a `MessageInt` / `MessageFloat` fluent alias set so
  idiomatic code reads `reflow.Int(42)` / `reflow.Str("x")`.
- **Python**: async `EventStream.recv_async` helper (currently users
  wrap `recv(timeout_ms)` with `asyncio.to_thread`).
- **JVM**: extend `NetworkConfig` constructor to accept a JSON string
  so the last remaining parity gap closes.
- **Kotlin**: add `suspend fun Message.toJson()` / `fromJson` that move
  serialization off the caller's thread; tiny, but removes the one
  blocking call on the hot path.

---

## Developer experience

- **Streams example in Node** — Rust bindings are complete but the
  `04_streams.mjs` smoke test currently exercises only the producer /
  consumer round trip. Add an example that runs a stream end-to-end
  through a network (source actor → stream → sink actor).
- **Per-SDK `examples/README.md`** — short overview of what each
  example demonstrates, with run instructions.
- **Central `sdk/README.md`** — landing page linking to each SDK's
  README, since the repo root currently points only at Rust crates.

---

## Recently shipped

- **2026-04-23** — Multi-graph composition in all four SDKs
  (`composeGraphs` / `ComposeGraphs` / `compose_graphs` /
  `MultiGraph.compose`).
- **2026-04-23** — Kotlin DSL + coroutine adapters on the JVM SDK.
- **2026-04-22** — Python, Go, Node, JVM first-cut SDKs with the full
  runtime surface (Message / Actor / Network / Graph / SubgraphBuilder
  / Stream / EventStream / template catalog).
- **2026-04-22** — C ABI surface frozen at `reflow_rt_capi`: typed
  messages, streams, subgraph builder, callback actors, bundled
  catalog, NetworkConfig JSON.

---

## How to propose an addition

The pattern across every SDK is:

1. Extend the Rust surface in `reflow_rt` / `reflow_network` if the
   primitive isn't already exposed.
2. Add a thin wrapper to `crates/reflow_rt_capi` — typically a single
   function or small handle type with a JSON-in, JSON-out contract.
3. Regenerate `crates/reflow_rt_capi/include/reflow_rt.h` with
   `cargo build -p reflow_rt_capi --features generate-header`.
4. Add the same binding in each SDK's native-bindings crate:
   - Node: `sdk/node/src/lib.rs` (napi-derive)
   - Go: `sdk/go/*.go` (cgo — links to the C ABI)
   - Python: `sdk/python/src/lib.rs` (pyo3)
   - JVM: `sdk/jvm/src/native/src/lib.rs` (jni) plus a Java class
5. Add a test per SDK under its `test/` / `tests/` directory that
   exercises the new surface end-to-end.
6. Update the SDK's README with a short usage example.

Every addition is therefore 5 roughly parallel commits plus the shared
Rust/C code. Keep scope tight per PR — one feature at a time.
