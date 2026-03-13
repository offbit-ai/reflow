# Reflow: Distributed Workflows & Subgraph Integration Gaps Plan

> Tracked plan for closing integration gaps between the distributed network layer and the subgraph/composition system.

## Phase 1: Router & Proxy Correctness Fixes ✅
*Low-risk, high-impact fixes to existing code that's already wired up but broken/incomplete.*

- [x] **1.1** `router.rs` — Fixed `source_actor` propagation. Added `source_actor: Option<&str>` through `route_message()` → `send_remote_message()` → proxy.
- [x] **1.2** `router.rs` — Fixed capabilities. `register_remote_actor()` now accepts `capabilities: Option<Vec<String>>`.
- [x] **1.3** `bridge.rs` — Fixed heartbeat using `config.network_id` instead of hardcoded `"local"`.
- [x] **1.4** `bridge.rs` — Fixed discovery response using `config.network_id` instead of `"local"`.
- [x] **1.5** `router.rs` — Removed no-op `set_connection_pool()` method.
- [x] **1.6** `multi_graph/mod.rs` — Implemented `handle_namespace_conflict()` with all three policies (Fail, VersionSuffix, AutoResolve).

## Phase 2: Discovery & Connection Reliability ✅
*Make the distributed layer self-healing and self-discovering.*

- [x] **2.1** `bridge.rs` — Discovery responses now processed: auto-registers actors or fulfils pending `discover_remote_actors()` calls.
- [x] **2.2** `bridge.rs` — `discover_remote_actors()` now uses oneshot channel + 10s timeout for async response handling.
- [x] **2.3** `bridge.rs` — `handle_discovery_request` now queries real actor list from local network via `router.get_local_actor_list()`.
- [x] **2.4** `bridge.rs` — Connection status transitions (Connected→Reconnecting→Failed) on disconnect/error before cleanup.
- [x] **2.5** `bridge.rs` — `ConnectionStatus` state machine now actively transitioned in message handler and heartbeat monitor.

## Phase 3: Proxy Actor Robustness ✅
*The proxy is fire-and-forget — make it production-grade.*

- [x] **3.1** `proxy.rs` — Errors now propagated to outports so downstream actors are notified (not just swallowed).
- [x] **3.2** `proxy.rs` — Structured error messages include proxy ID and failure reason (e.g., `"proxy:actor@net: forward failed: ..."`)
- [x] **3.3** `proxy.rs` — 30-second timeout on `send_remote_message()` via `tokio::time::timeout`. Timeout errors propagated to outports.

## Phase 4: Distributed + Subgraph Integration Bridge ✅
*The architectural gap — connecting composition with distribution.*

- [x] **4.1** New module `distributed_composition.rs` — `DistributedGraphComposition`, `RemoteGraphConfig`, `DistributedConnection`, `DistributedEndpoint`.
- [x] **4.2** `DistributedNamespaceResolver` — three-level namespace `network_id/namespace/process` with local/remote registration and resolution.
- [x] **4.3** `plan_distributed_composition()` auto-generates `ProxyActorSpec` for cross-network edges. `execute_distributed_plan()` creates the proxies.
- [x] **4.4** `execution_targets: HashMap<String, String>` in `DistributedGraphComposition` declares which network runs which subgraph.

## Phase 5: Testing & Validation ✅
*18 tests passing across all categories.*

- [x] **5.1** Router unit tests: register with default/custom capabilities, route with no connection, incoming with no network, actor list empty.
- [x] **5.2** *(Deferred — requires two live network instances; covered by existing `distributed_example.rs`)*
- [x] **5.3** Composition test: two graphs with namespace isolation + cross-graph connections verified.
- [x] **5.4** Distributed composition tests: namespace resolver (local/remote), cross-network edge detection, plan generation, proxy deduplication, endpoint qualified names.
- [x] **5.5** All three `NamespaceConflictPolicy` variants tested (Fail rejects, AutoResolve assigns different, VersionSuffix resolves).
- [x] **5.6** Circular dependency detection: linear ordering verified, cycles detected and reported.

## Files Changed

| File | Changes |
|------|---------|
| `router.rs` | `source_actor` param, capabilities param, `get_local_actor_list()`, removed `set_connection_pool()` |
| `bridge.rs` | Config-based network_id, discovery response processing, oneshot-based `discover_remote_actors()`, status transitions, `pending_discovery` field |
| `proxy.rs` | `proxy_id` field, timeout on forwarding, structured error propagation to outports |
| `distributed_network.rs` | Updated `send_to_remote_actor` with `source_actor`, added `register_remote_actor_with_capabilities` |
| `multi_graph/mod.rs` | Implemented `handle_namespace_conflict()` for all three policies |
| `distributed_composition.rs` | **NEW** — distributed+subgraph integration bridge |
| `integration_tests.rs` | **NEW** — 18 integration tests |
| `.cargo/config.toml` | Removed broken LLVM linker override |
