# reflow_litert

LiteRT backend boundary for Reflow — the stable trait surface ML actors use to run on-device inference, plus an optional real LiteRT adapter.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt) with the `ml` feature**, which re-exports this crate as `reflow_rt::litert`. To enable real LiteRT execution, additionally opt into `external-litert`.

## What it provides

- `LiteRtBackend` trait — the ML inference adapter surface.
- Mock implementation used in tests and in graphs that want deterministic behavior without pulling the LiteRT runtime.
- Optional `external-litert` feature — binds to the real LiteRT runtime for on-device acceleration.

## Why it is separate

Keeping LiteRT behind its own crate (and behind an opt-in feature) means:

- `reflow_rt` without `external-litert` compiles cleanly on any target.
- ML graphs remain testable without hardware.
- Release users who need real inference enable exactly one feature.

## License

MIT OR Apache-2.0.
