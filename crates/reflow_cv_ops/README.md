# reflow_cv_ops

CV preprocessing actors for Reflow — resize, normalize, colorspace convert, letterbox — feeding detectors and classifiers through the standard typed packet schema.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt) with the `ml` feature**, which re-exports this crate as `reflow_rt::cv_ops`. Direct use is appropriate when assembling CV pipelines without the full bundled runtime.

## What it provides

- Actor templates like `tpl_cv_preprocess`, frame-resize helpers, normalization, letterbox padding.
- Packets conform to `reflow_media_types` so they compose with `reflow_ml_ops` and decode actors.

## Typical pipeline

```text
frame -> tpl_cv_preprocess -> tpl_ml_inference -> tpl_detection_decode -> output
```

## License

MIT OR Apache-2.0.
