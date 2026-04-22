# reflow_actor_macro

Procedural macros for declaring Reflow actors.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt) and import these from `reflow_rt::prelude`.** This crate exists on its own so internal Reflow crates can author actors without pulling the full runtime facade. Direct dependency is only recommended for advanced users writing actor libraries intended to ship beside Reflow.

## What it provides

- `#[actor(...)]` — declare an async actor function along with its in/out ports, backpressure, and state type. Generates the actor struct, `Actor` trait impl, and registration glue.
- `#[actor_display]` — attach human-readable metadata to an actor for visual editors.

## Quick glance

```rust
use reflow_rt::prelude::*;
use anyhow::Error;
use std::collections::HashMap;

#[actor(
    AddActor,
    inports::<50>(a, b),
    outports::<50>(sum),
    state(MemoryState)
)]
pub async fn add_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let a = p.get("a").and_then(|m| m.as_f64()).unwrap_or(0.0);
    let b = p.get("b").and_then(|m| m.as_f64()).unwrap_or(0.0);
    Ok(HashMap::from([("sum".into(), Message::Float(a + b))]))
}
```

## License

MIT OR Apache-2.0.
