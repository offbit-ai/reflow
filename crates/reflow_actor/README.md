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

Most actors are declared with `#[actor(...)]` from `reflow_actor_macro`. It
generates the `Actor` impl, port setup, and registration glue.

```rust
use reflow_actor::{ActorContext, MemoryState, message::Message};
use reflow_actor_macro::actor;
use anyhow::Error;
use std::collections::HashMap;

/// Doubles every `number` packet and emits the result on `doubled`.
#[actor(
    DoubleActor,
    inports::<50>(number),
    outports::<50>(doubled),
    state(MemoryState)
)]
pub async fn double_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let n = ctx
        .get_payload()
        .get("number")
        .and_then(|m| m.as_f64())
        .unwrap_or(0.0);

    Ok(HashMap::from([
        ("doubled".into(), Message::Float(n * 2.0)),
    ]))
}
```

Key ideas the example shows:

- **Named ports.** `inports` / `outports` declare the schema; the network
  routes packets to this actor only on listed ports.
- **Selective emission.** Returning `{"doubled": …}` means "emit one packet on
  the `doubled` port". Return an empty `HashMap` to consume inputs silently.
- **State.** `state(MemoryState)` gives each actor instance its own mutable
  scratch area; swap for a custom `ActorState` to persist across ticks.
- **Backpressure.** `inports::<50>` sizes the per-port channel — bound it low
  for latency-sensitive flows, high for burst-friendly pipelines.

Use `StreamHandle` when moving large or continuous payloads (e.g. raw video
frames) — handles are cheap to copy through the message bus while the bytes
travel out-of-band.

## Relationship to other crates

- Used by every Reflow actor, including all actors in `reflow_components`, `reflow_api_services`, `reflow_ml_ops`, `reflow_cv_ops`.
- Paired with [`reflow_actor_macro`](https://docs.rs/reflow_actor_macro) for declaring actors ergonomically.

## License

MIT OR Apache-2.0.
