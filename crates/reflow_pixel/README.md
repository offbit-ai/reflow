# reflow_pixel

Pure-Rust pixel operations for Reflow image and video actors — resize, convert, blend, channel swizzle. Wasm-safe.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt)** which re-exports this crate as `reflow_rt::pixel`. Direct use is appropriate when writing custom image-processing actors that target both native and Wasm.

## What it provides

- RGBA / RGB / grayscale pixel helpers without pulling a native image codec.
- Format conversions used internally by `reflow_components` image and video actors.
- Allocation-conscious helpers suitable for per-frame work.

Codec support (PNG / JPEG decode/encode) lives in actors inside `reflow_components`, not here — this crate is deliberately the lower-level primitive surface.

## License

MIT OR Apache-2.0.
