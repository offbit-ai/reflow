# Reflow LiteRT Hand Landmark Demo

This is a native-only Reflow network demo for the real LiteRT backend using the official MediaPipe hand model assets. It does not use browser camera capture yet.

The demo proves:

- Reflow can depend on Offbit LiteRT from Git.
- Model manifests can describe real LiteRT tensor signatures.
- `Network` and actors carry model bytes, tensor packets, inference outputs, and decoded landmarks through ports.
- `RunInferenceActor` loads and runs the palm detector and hand landmark models through the real LiteRT backend.
- Model binaries stay outside Git while checksums remain reproducible.
- The demo crate embeds the LiteRT runtime rpath on macOS through `build.rs`.

## Model Assets

Fetch the models into `examples/ml_hand_landmark_demo/models`:

```bash
examples/ml_hand_landmark_demo/scripts/fetch_models.sh
```

The expected assets are:

| File | Source | SHA-256 |
| --- | --- | --- |
| `palm_detection_full.tflite` | `https://storage.googleapis.com/mediapipe-assets/palm_detection_full.tflite` | `1b14e9422c6ad006cde6581a46c8b90dd573c07ab7f3934b5589e7cea3f89a54` |
| `hand_landmark_full.tflite` | `https://storage.googleapis.com/mediapipe-assets/hand_landmark_full.tflite` | `11c272b891e1a99ab034208e23937a8008388cf11ed2a9d776ed3d01d0ba00e3` |

The current signatures are captured in `manifests/`:

- Palm detector input: `f32[1, 192, 192, 3]`
- Palm detector outputs: `f32[1, 2016, 18]`, `f32[1, 2016, 1]`
- Hand landmark input: `f32[1, 224, 224, 3]`
- Hand landmark outputs: `f32[1, 63]`, `f32[1, 1]`, `f32[1, 1]`, `f32[1, 63]`

## Run

```bash
cargo run --manifest-path examples/ml_hand_landmark_demo/Cargo.toml --release
```

Or pass a custom model directory:

```bash
cargo run --manifest-path examples/ml_hand_landmark_demo/Cargo.toml --release -- \
  --models-dir /path/to/models
```

## Graph

```text
IIP model bytes + SyntheticTensorSourceActor
  -> RunInferenceActor
  -> InferenceSummaryActor

IIP model bytes + SyntheticTensorSourceActor
  -> RunInferenceActor
  -> DecodeLandmarksActor
  -> LandmarkPreviewActor
```

The model bytes are initial packets because model files are external assets. Tensor generation, inference, decoding, and summaries all run as actors inside `Network`.

## Scope

This is intentionally a native Reflow actor/network demo, not the final hand-tracking UX. A full interactive demo still needs camera-frame preprocessing, palm anchor decode/NMS, ROI tracking, and overlay rendering wired as graph actors.
