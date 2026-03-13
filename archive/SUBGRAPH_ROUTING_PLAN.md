# Reflow: First-Class Subgraph Support & Distributed-to-Subgraph Routing

> Phased plan for making subgraphs a runtime-first concept with hierarchical addressing and distributed bridging.

## Architecture Context

**Current state**: Graphs compose side-by-side (namespace/process), but flatten at runtime — no parent-child hierarchy. Distributed routing is flat actor IDs. `GraphExport.inports`/`outports` exist but are unused at runtime.

**Target state**: A graph node can BE another graph. Messages route hierarchically through subgraph boundaries. Distributed networks can target actors inside subgraphs and vice versa.

## Phase 1: SubgraphActor — A Graph as a First-Class Actor
*Create a `SubgraphActor` that wraps an inner `Network` and routes through inports/outports boundaries.*

- [ ] **1.1** New `subgraph.rs` — `SubgraphActor` struct: owns inner `Network`, `inport_map: HashMap<String, (actor_id, port)>`, `outport_map`, implements `Actor` trait.
- [ ] **1.2** `SubgraphActor::from_graph_export()` — constructs inner network from `GraphExport`, builds inport/outport maps from `GraphExport.inports`/`GraphExport.outports`.
- [ ] **1.3** `SubgraphActor::create_process()` — spawns inbound loop (external inports → inner `send_to_actor`) and outbound loop (inner actor outports → external outports).
- [ ] **1.4** `SubgraphActor` registered in parent network via `register_actor_arc()` — no changes to `Network` in this phase.
- [ ] **1.5** Add `pub mod subgraph;` to `lib.rs`.
- [ ] **1.6** Tests: parent network with SubgraphActor node, message flows through inport → inner actor → outport → parent.

## Phase 2: Hierarchical Actor Addressing
*`send_to_actor("subgraph_a/actor_b", port, msg)` resolves through graph boundaries. Flat IDs still work.*

- [ ] **2.1** New `address.rs` — `ActorAddress` with `parse(path)`, `is_local()`, `head()`, `tail()`.
- [ ] **2.2** `network.rs` — Add `subgraphs: HashMap<String, Arc<SubgraphActor>>` field to `Network`. Populated on `add_node()` when actor is `SubgraphActor`.
- [ ] **2.3** `network.rs` — Extend `send_to_actor()`: if flat lookup fails AND id contains `/`, parse as `ActorAddress`, resolve first segment from `subgraphs`, delegate remaining path via `SubgraphActor::route_to_inner()`.
- [ ] **2.4** `subgraph.rs` — Add `route_to_inner(path, port, data)` method that calls `inner_network.send_to_actor(path, port, data)` — recursion handles arbitrary depth.
- [ ] **2.5** `connector.rs` — `Connector::init()` supports hierarchical addresses: walks subgraph chain to resolve actual inport/outport channels.
- [ ] **2.6** Tests: `send_to_actor("sub/inner_actor", ...)`, deep nesting `send_to_actor("a/b/c", ...)`, flat IDs still work.

## Phase 3: Subgraph-Aware Graph Composition
*`GraphComposer` natively creates `SubgraphActor` instances. Graph JSON can declare subgraph nodes.*

- [ ] **3.1** `types.rs` — Extend `GraphNode` with `subgraph: Option<Box<GraphExport>>` (serde-optional).
- [ ] **3.2** `multi_graph/mod.rs` — Add `compose_to_network()` method: detects nodes with `subgraph` set, creates `SubgraphActor` instead of expecting a pre-registered actor. Regular nodes registered normally.
- [ ] **3.3** Support reference-based subgraphs: if `component` matches a known graph name in the composition, resolve it as a subgraph reference.
- [ ] **3.4** Cycle detection: verify subgraph references don't create cycles (A → B → A).
- [ ] **3.5** Tests: compose graphs where one node is a subgraph, verify end-to-end message flow through composition.

## Phase 4: Distributed ↔ Subgraph Bridging
*Remote networks target subgraph actors. Subgraph actors send outward to distributed actors.*

- [ ] **4.1** `router.rs` — No structural change needed: `handle_incoming_message()` already calls `send_to_actor(target_actor, ...)`. With Phase 2 in place, hierarchical `target_actor` paths route automatically.
- [ ] **4.2** `distributed_composition.rs` — `DistributedEndpoint.process` now supports hierarchical paths (`namespace/subgraph/actor`). `register_local_subgraph()` walks subgraph hierarchies.
- [ ] **4.3** `proxy.rs` — `remote_actor_id` already a String, hierarchical paths work. Verify proxy naming convention for subgraph targets (e.g., `"sub/actor@remote_net"`).
- [ ] **4.4** `subgraph.rs` — New `SubgraphOutboundProxy`: lives inside inner network, forwards messages to parent network. Registered during `SubgraphActor::from_graph_export()` for each outport.
- [ ] **4.5** Tests: remote network sends to `network_b/subgraph_x/actor_y`. Actor inside subgraph sends to remote network actor. Bidirectional flow.

## Phase 5: Lifecycle, Nesting, and Event Propagation
*Arbitrary nesting depth, cascading start/stop, event bubbling.*

- [ ] **5.1** `network.rs` — `Network::start()` cascades to subgraph inner networks. `Network::shutdown()` tears down inner networks first (bottom-up).
- [ ] **5.2** `subgraph.rs` — `start_inner()` / `shutdown_inner()` lifecycle methods.
- [ ] **5.3** Max depth enforcement: `NetworkConfig.max_subgraph_depth` (default: 8). Validated during `SubgraphActor` construction.
- [ ] **5.4** Event bubbling: inner network events forwarded to parent, prefixed with subgraph ID. New `NetworkEvent::SubgraphEvent { subgraph_id, inner_event }` variant.
- [ ] **5.5** Tests: 3-level nesting, cycle rejection, shutdown cascading, event bubbling from nested actor to top-level receiver.

## Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| SubgraphActor owns inner `Network` (not flattened) | Preserves boundary encapsulation, enables lifecycle control, maintains isolation |
| Hierarchical addressing via `/` parsing | Backward compatible — existing `namespace/process` flat keys still resolve via HashMap |
| Proxy pattern extended for subgraph boundaries | `SubgraphOutboundProxy` mirrors `RemoteActorProxy` — proven pattern |
| `GraphExport.inports`/`outports` as runtime boundary | Already defined in types, now activated at runtime instead of discarded after composition |

## Files Changed/Created

| File | Phase | Changes |
|------|-------|---------|
| `subgraph.rs` | 1,2,4,5 | **NEW** — SubgraphActor, SubgraphOutboundProxy, lifecycle |
| `address.rs` | 2 | **NEW** — ActorAddress hierarchical path parsing |
| `network.rs` | 2,3,5 | `subgraphs` field, hierarchical `send_to_actor`, lifecycle cascading |
| `types.rs` | 3 | `GraphNode.subgraph` optional field |
| `multi_graph/mod.rs` | 3 | `compose_to_network()`, subgraph detection, cycle detection |
| `connector.rs` | 2 | Hierarchical address resolution in `init()` |
| `distributed_composition.rs` | 4 | Subgraph-aware namespace registration |
| `proxy.rs` | 4 | Verify hierarchical target ID handling |
| `router.rs` | 4 | Works automatically with Phase 2 — validate |
| `lib.rs` | 1 | Add `pub mod subgraph; pub mod address;` |
