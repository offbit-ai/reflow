# reflow_api_services

Generated API-service actor catalog for Reflow — ~6700 ready-to-wire actors covering ~90 third-party services (payments, messaging, LLMs, data, analytics, maps, weather, and more).

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt) with the `api-services` feature**, which re-exports this crate as `reflow_rt::api_services` and registers the actors through `reflow_components`. Direct use is appropriate when embedding only the API surface without the rest of the component catalog.

## What it provides

- One actor per service endpoint (e.g. `tpl_openai_chat_completion`, `tpl_stripe_create_customer`, `tpl_github_create_issue`).
- Per-actor schema derived from the upstream API — input/output ports map to request/response fields.
- A large registry catalog that editors can introspect to surface every available action.

## Typical usage

```rust
use reflow_rt::prelude::*;

net.register_actor_arc(
    "tpl_openai_chat_completion",
    get_actor_for_template("tpl_openai_chat_completion").unwrap(),
)?;
net.add_node("llm", "tpl_openai_chat_completion", /* config with api_key, model, … */ None)?;
```

## Notes on size

The catalog is large. If you only need a subset, prefer depending on `reflow_rt` with `api-services` enabled and using the runtime template registry to register only the actors you add to the graph.

## License

MIT OR Apache-2.0.
