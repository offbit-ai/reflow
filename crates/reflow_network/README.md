# reflow_network

The Reflow network executor — owns actors, routes messages, manages subgraphs, and emits runtime events.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt)** which re-exports this crate as `reflow_rt::network`. Depend directly on `reflow_network` only if you are embedding the executor in a custom runtime and do not want the bundled component catalog.

## What it provides

- `Network`, `NetworkConfig` — the executable runtime.
- `SubgraphActor` — embed a `GraphExport` as a first-class actor in a parent network.
- `NetworkEvent` stream — `ActorStarted`, `ActorFailed`, packet-level trace events.
- `TemplateRegistry`, `NodeTemplate`, `DisplayComponent` — discovery surface for editors.
- Optional feature `flowtrace` for structured flow tracing.
- Optional feature `wasm` for browser / Wasm runtimes.

## Quick glance

```rust
use reflow_network::network::{Network, NetworkConfig, NetworkEvent};

let mut net = Network::new(NetworkConfig::default());
// ... register actors, add_node, add_connection, add_initial ...

let events = net.get_event_receiver();
tokio::spawn(async move {
    while let Ok(evt) = events.recv_async().await {
        if let NetworkEvent::ActorFailed { actor_id, error, .. } = &evt {
            eprintln!("fail {actor_id}: {error}");
        }
    }
});

net.start()?;
```

Always subscribe to events **before** calling `start()` so no events are missed.

## License

MIT OR Apache-2.0.
