# reflow_ml_ops

ML inference and decode actors for Reflow — runs model manifests through the LiteRT boundary, decodes tensors into typed detection / landmark / ROI packets.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt) with the `ml` feature**, which re-exports this crate as `reflow_rt::ml_ops`. Direct use is appropriate when assembling ML pipelines without the full bundled runtime.

## What it provides

- Inference actor driven by model manifests from `reflow_asset_registry`.
- Detection, classification, and landmark decode actors producing `reflow_media_types` packets.
- Swappable backend via `reflow_litert` — mock by default, real LiteRT with `external-litert`.

## Typical pipeline

```text
preprocessed_tensor -> tpl_ml_inference -> decode -> taskpack -> output
```

Model behavior comes from manifests and per-node configuration. Taskpacks are reusable graph exports, not privileged runtime code.

## License

MIT OR Apache-2.0.
