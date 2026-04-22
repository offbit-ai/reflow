# reflow_actor

Actor trait, message types, ports, state, and stream handles used by the Reflow runtime.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt)** which re-exports this crate as `reflow_rt::actor_runtime` and exposes the common types through `reflow_rt::prelude`. Depend directly on `reflow_actor` only if you are writing an actor library or a runtime component that must not pull in the whole facade.

## What it provides

- `Actor` trait, `ActorBehavior`, `ActorContext`, `ActorConfig`, `ActorLoad`, `ActorPayload`.
- Message types: `Message`, `EncodableValue`.
- Port types: `Port`, `ActorChannel`.
- State: `ActorState`, `MemoryState`.
- Streams: `StreamHandle`, `StreamFrame`, and the stream registry for out-of-band frame transport.

## Quick glance

```rust
use reflow_actor::{ActorContext, message::Message};
use std::collections::HashMap;

async fn passthrough(ctx: ActorContext) -> anyhow::Result<HashMap<String, Message>> {
    let inputs = ctx.get_payload();
    Ok(inputs.clone())
}
```

Use `StreamHandle` when moving large or continuous payloads (e.g. raw video frames) — handles are cheap to copy through the message bus while the bytes travel out-of-band.

## Relationship to other crates

- Used by every Reflow actor, including all actors in `reflow_components`, `reflow_api_services`, `reflow_ml_ops`, `reflow_cv_ops`.
- Paired with [`reflow_actor_macro`](https://docs.rs/reflow_actor_macro) for declaring actors ergonomically.

## License

MIT OR Apache-2.0.
