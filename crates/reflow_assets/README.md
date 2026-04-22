# reflow_assets

Content-addressed asset database conventions for Reflow — the `aid://blake3/…` URI scheme plus typed metadata and content kinds used across the runtime.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt)** which re-exports this crate as `reflow_rt::assets`. Direct use is only needed for custom tools that author, index, or serve Reflow assets.

## What it provides

- Asset identifiers and URI parsing.
- Content-kind enumeration for meshes, textures, tensors, audio, etc.
- Shared types consumed by `reflow_asset_registry` (for the registry client) and by asset-producing / consuming actors in `reflow_components`.

## Quick glance

```rust
use reflow_assets::{Asset, AssetId};

let aid: AssetId = "aid://blake3/…".parse()?;
let asset = Asset::new(aid, /* kind */ reflow_assets::ContentKind::Mesh);
```

Pure-Rust and Wasm-safe — no filesystem, network, or codec dependencies by default.

## License

MIT OR Apache-2.0.
