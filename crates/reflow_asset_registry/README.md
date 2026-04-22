# reflow_asset_registry

Asset registry client for Reflow — resolves content-addressed `aid://` URIs, pulls model manifests and weights, and hands them to ML actors.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt) with the `ml` feature**, which re-exports this crate as `reflow_rt::asset_registry`. Direct use is appropriate only when embedding Reflow's asset resolution into a non-ML application.

## What it provides

- Registry client that resolves assets described in `reflow_assets`.
- Manifest types for ML models consumed by `reflow_ml_ops`.
- Deterministic mockable behavior — the registry never pulls native runtimes by itself.

Real LiteRT execution is orthogonal and opt-in through `reflow_litert`'s `external-litert` feature.

## License

MIT OR Apache-2.0.
