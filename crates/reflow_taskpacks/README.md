# reflow_taskpacks

Reusable Reflow graph packages for higher-level workflows — object detection, pose estimation, hand/face landmarks, media transcoding.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt) with the `ml` feature**, which re-exports this crate as `reflow_rt::taskpacks`. Direct use is appropriate for authoring new taskpacks or shipping them as standalone packages.

## What a taskpack is

A taskpack is a serialized `GraphExport` (plus metadata) that wires together preprocess, inference, and decode actors into a reusable subgraph. You can embed it into a parent graph with `SubgraphActor` and expose only the ports your app touches.

Because taskpacks are just graph data, they:

- Can be versioned, downloaded, and swapped at runtime.
- Get validated against the standard template registry.
- Require no framework-level hardcoded pipelines.

## Relationship to other crates

- Graph shape comes from `reflow_graph`.
- Actors inside taskpacks come from `reflow_cv_ops`, `reflow_ml_ops`, and `reflow_components`.
- Executed by `reflow_network` (or through `reflow_rt`).

## License

MIT OR Apache-2.0.
