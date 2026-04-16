# Media / ML Stack

Reflow's Media / ML stack is a graph-driven layer for image, tensor, inference, and taskpack workflows. It is designed for MediaPipe-class pipelines without copying MediaPipe internals or baking model-specific behavior into the runtime.

The stack keeps the same Reflow principles as the rest of the component system:

- Packets move through ordinary DAG edges as `Message::Bytes`, `Message::StreamHandle`, `Message::Object`, or `Message::Encoded`.
- Synchronization uses actor-level `await_inports(...)` instead of special runtime message variants.
- Model behavior comes from manifests, node config, tensor specs, ROI rules, thresholds, and decode parameters.
- Taskpacks are reusable `GraphExport` subgraphs, not privileged runtime code.
- Native inference backends are optional adapters behind `reflow_litert::InferenceBackend`.

## Crates

| Crate | Role |
| --- | --- |
| `reflow_media_types` | Shared packet contracts for frames, tensors, detections, landmarks, ROI, timestamps, and metadata. |
| `reflow_media_codec` | Helpers for converting media and tensor packets to and from existing Reflow messages. |
| `reflow_asset_registry` | Model manifest validation and local asset loading on top of the existing asset database conventions. |
| `reflow_cv_ops` | Graph-friendly CV preprocess actors such as image-to-tensor, resize/letterbox, normalization, ROI crop, detection-to-ROI, and smoothing. |
| `reflow_ml_ops` | ML actors for loading model metadata/assets, running inference, decoding detections/landmarks, and probing packets. |
| `reflow_litert` | Backend boundary plus deterministic mock inference by default. Optional real LiteRT support is enabled with `external-litert`. |
| `reflow_taskpacks` | Reusable taskpack graph builders, including the V1 hand-landmark-style pipeline. |

## LiteRT Integration

Reflow owns the backend boundary, not the native LiteRT implementation. By default, `reflow_litert` uses `MockBackend`, which keeps graph authoring, examples, and non-ML workflows free from native ML runtime requirements.

Real LiteRT execution is available through the optional `external-litert` feature. The adapter targets Offbit's separate LiteRT Rust bindings, which will be published at `https://github.com/offbit-ai/LiteRT` when stable. With that feature enabled, models configured with `backend: "litert"` are loaded through the LiteRT adapter. Without it, Reflow returns a clear graph error instead of silently falling back to mock inference.

The graph contract does not change when switching from mock inference to LiteRT. Frames, tensors, model metadata, ROI packets, and decoded outputs continue to move through the same actors and ports.

## Authoring Model

Graph authors should treat ML pipelines like any other Reflow graph:

```text
frame
  -> preprocess
  -> load model
  -> run inference
  -> decode
  -> postprocess / smooth
  -> output
```

The inference actor does not know about hand landmarks, palm detection, or any other specific model family. Those details belong in model manifests, node config, or taskpack graph composition.

## Feature Flags

`reflow_components` exposes the catalog surface through the optional `ml` feature:

```toml
reflow_components = { path = "../reflow_components", features = ["ml"] }
```

The native LiteRT adapter is separate:

```toml
reflow_ml_ops = { path = "../reflow_ml_ops", features = ["external-litert"] }
```

This split lets existing users opt into ML/CV graph templates without inheriting native LiteRT build and runtime requirements unless they explicitly need real LiteRT execution.
