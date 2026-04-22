# reflow_media_types

Typed packet types used by Reflow's media and ML pipelines — frames, tensors, detections, landmarks, timestamps, ROIs.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt) with the `media` feature**, which re-exports this crate as `reflow_rt::media_types`. Direct use is appropriate when writing custom media or ML actors that must share the standard packet shapes.

## What it provides

- `Frame` — RGBA / RGB / YUV frame descriptors with metadata and timestamps.
- `Tensor` — dtype and shape for ML input / output tensors.
- `Detection`, `Landmark`, `Roi` — typed CV output packets.
- `Timestamp`, `FrameId` — monotonic stream identifiers.

Typed packets give every actor a stable schema so detectors, decoders, and taskpacks can be swapped without changing wiring.

## License

MIT OR Apache-2.0.
