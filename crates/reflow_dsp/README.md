# reflow_dsp

Pure-Rust DSP primitives for Reflow audio and signal-processing actors — biquad filters, compressors, envelope followers, FFT helpers, resampling. Wasm-safe.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt) with the `av-core` feature**, which re-exports this crate as `reflow_rt::dsp`. Direct use is appropriate when building custom DSP actors that target both native and Wasm.

## What it provides

- Biquad filters, one-pole filters, parametric EQ.
- Dynamics processors (compressor, limiter, noise gate).
- Envelope follower, peak detect, silence detect, DC offset.
- Sample-rate-aware helpers used by audio actors in `reflow_components`.

## Relationship to other crates

- Consumed exclusively by audio `stream_ops` actors in `reflow_components` behind the `av-core` feature.

## License

MIT OR Apache-2.0.
