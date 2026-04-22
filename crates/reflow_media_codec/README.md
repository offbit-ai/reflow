# reflow_media_codec

Media codec boundary for Reflow — decode/encode adapters shared by media and ML actors.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt) with the `media` feature**, which re-exports this crate as `reflow_rt::media_codec`. Direct use is appropriate when writing custom codec adapters or offline media tools.

## What it provides

- Codec trait surface consumed by `reflow_cv_ops` (preprocess) and `reflow_ml_ops` (decode outputs).
- Decoder / encoder wiring that actors can swap without changing graph topology.

## Relationship to other crates

- Bridges `reflow_media_types` packets to the actual byte-level representation used by codecs.
- Consumed by `reflow_cv_ops`, `reflow_ml_ops`, and several `reflow_components` stream actors.

## License

MIT OR Apache-2.0.
