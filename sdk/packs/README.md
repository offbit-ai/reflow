# Reflow Actor Pack Catalog

A `.rflpack` is a multi-platform zip bundle containing one or more
native cdylibs and a manifest. SDKs load packs at runtime to extend the
template catalog with additional actors — no SDK rebuild required.

## Where to download

First-party packs ship as assets on every [GitHub Release](https://github.com/offbit-ai/reflow/releases)
whose tag starts with `pack-v`. Each `.rflpack` is a single zip
containing the cdylib for every triple the pack supports plus the
manifest:

| Triple | Notes |
|---|---|
| `aarch64-apple-darwin` | every pack |
| `x86_64-apple-darwin` | every pack |
| `x86_64-unknown-linux-gnu` | every pack |
| `aarch64-unknown-linux-gnu` | every pack |
| `x86_64-pc-windows-msvc` | every pack |
| `wasm32-unknown-unknown` | every pack except `browser` (which drives a real Chrome instance over TCP via CDP — impossible from inside a browser tab). Native rendering uses `wgpu`'s WebGPU backend; HTTP uses Fetch via `reqwest`'s wasm support; H.264 video encoding falls back to the [WebCodecs `VideoEncoder`](https://developer.mozilla.org/en-US/docs/Web/API/VideoEncoder) API (Chromium / Edge / Safari ship it; Firefox-on-Android does not). Native loaders ignore the `.wasm` entry; the browser-side pack loader in `@offbit-ai/reflow` picks it up via `WebAssembly.instantiate`. |

## Pack ABI

A pack `cdylib`'s exported symbols depend on the target — the
`#[reflow_pack]` macro picks the right shape automatically.

### Native (`dlopen` / `LoadLibrary`)

| Symbol | Direction | Purpose |
|---|---|---|
| `reflow_pack_abi_version() -> u32` | export | ABI handshake |
| `reflow_pack_register(host: *mut PackHostVtable) -> i32` | export | Loader passes a vtable; pack registers templates by calling vtable function pointers |

### Wasm (`WebAssembly.instantiate`)

| Symbol | Direction | Purpose |
|---|---|---|
| `reflow_pack_abi_version() -> u32` | export | ABI handshake (same as native) |
| `__reflow_pack_register() -> i32` | export | Loader calls once after instantiate; pack walks its register fn and emits one `__reflow_pack_register_template` import per template |
| `__reflow_pack_create_actor(factory_id: u32) -> *mut PackActorHandle` | export | Loader calls per actor instantiation |
| `env.__reflow_pack_register_template(name_ptr, name_len, factory_id)` | **import** | Loader provides; pack calls once per registered template during `__reflow_pack_register` |

The ABI version is computed identically on both sides (FNV-1a hash
of the rustc verbose version + a manually bumped revision in
`reflow_pack_loader/build.rs`). A pack built against an older
toolchain than the runtime is rejected at load time on either ABI.

```sh
VER=0.2.0
curl -LO https://github.com/offbit-ai/reflow/releases/download/pack-v$VER/reflow.pack.ml-$VER.rflpack
```

Then, from any SDK:

```js
require('@offbit-ai/reflow').loadPack('./reflow.pack.ml-0.2.0.rflpack');
```

```python
import offbit_reflow; offbit_reflow.load_pack('./reflow.pack.ml-0.2.0.rflpack')
```

```go
reflow.LoadPack("./reflow.pack.ml-0.2.0.rflpack")
```

```kotlin
ai.offbit.reflow.Packs.loadPack("./reflow.pack.ml-0.2.0.rflpack")
```

## Catalog

Each row links to the source crate and lists the templates the pack
publishes. Counts and template ids match the bundled
`reflow_components` registry.

### [`reflow.pack.browser`](browser/)

Browser automation backed by [chromiumoxide](https://crates.io/crates/chromiumoxide).
Drives a headless Chrome/Chromium instance and emits screencast frames.

| Template | Actor | Notes |
|----------|-------|-------|
| `tpl_browser_screencast` | `BrowserScreencastActor` | Connects to a running Chrome via CDP and streams JPEG frames |

### [`reflow.pack.video_encode`](video_encode/)

H.264 video encoding via [openh264](https://crates.io/crates/openh264).
Pure-Rust software encoder, no external libs.

| Template | Actor | Notes |
|----------|-------|-------|
| `tpl_video_encoder` | `VideoEncoderActor` | RGBA frames → H.264 NAL stream |

### [`reflow.pack.ml`](ml/)

Computer-vision ops + LiteRT inference. Default backend is the
mock-safe LiteRT shim shipped with `reflow_litert`; for hardware
inference, build with the `external-litert` Cargo feature to link
against a system LiteRT.

| Template | Actor | Notes |
|----------|-------|-------|
| `tpl_cv_image_to_tensor` | `ImageToTensorActor` | RGBA frame → NCHW tensor |
| `tpl_cv_resize_letterbox` | `ResizeLetterboxActor` | Aspect-preserving resize for detector inputs |
| `tpl_cv_video_stream_to_frames` | `VideoStreamToFramesActor` | Frame source for downstream ML graphs |
| `tpl_cv_normalize_tensor` | `NormalizeTensorActor` | Per-channel mean/std normalization |
| `tpl_cv_tensor_crop_roi` | `TensorCropRoiActor` | Crop a region from a tensor by ROI box |
| `tpl_cv_detection_to_roi` | `DetectionToRoiActor` | Convert detector output → ROI box |
| `tpl_cv_temporal_smoother` | `TemporalSmootherActor` | One-Euro filter for landmark stability |
| `tpl_ml_load_model` | `LoadModelActor` | Loads an asset-DB-resolved `.tflite` model |
| `tpl_ml_run_inference` | `RunInferenceActor` | Tensor in → tensor out, sync inference |
| `tpl_ml_decode_detections` | `DecodeDetectionsActor` | Post-processes object-detection tensors |
| `tpl_ml_decode_landmarks` | `DecodeLandmarksActor` | Post-processes landmark tensors |
| `tpl_ml_packet_probe` | `PacketProbeActor` | Per-tick latency / shape inspector |

### [`reflow.pack.gpu`](gpu/)

wgpu-backed GPU compute and rendering — SDF marching cubes, scene
rendering, 2D rasterizer.

| Template | Actor | Notes |
|----------|-------|-------|
| `tpl_sdf_live_render` | `SdfLiveRenderActor` | Live SDF ray-march to a render target |
| `tpl_sdf_render` | `SdfRenderActor` | Final-quality SDF render |
| `tpl_sdf_marching_cubes` | `SdfMarchingCubesActor` | SDF → triangle mesh |
| `tpl_mesh_to_sdf` | `MeshToSdfActor` | Triangle mesh → SDF voxel grid |
| `tpl_scene_render` | `SceneRenderActor` | Forward renderer with skinning + materials |
| `tpl_gpu_2d_render` | `Gpu2DRenderActor` | tiny-skia-style 2D vector raster on GPU |

### [`reflow.pack.window_events`](window_events/)

Input event sources. Wired to the host window system in production
SDKs; emits Reflow messages on every input event.

| Template | Actor | Notes |
|----------|-------|-------|
| `tpl_keyboard_input` | `KeyboardInputActor` | Keydown/keyup/repeat events |
| `tpl_mouse_input` | `MouseInputActor` | Mouse move + button events |
| `tpl_gamepad_input` | `GamepadInputActor` | Standard-mapping gamepad axes/buttons |
| `tpl_touch_input` | `TouchInputActor` | Multi-touch points |
| `tpl_window_event` | `WindowEventActor` | Resize, focus, close, drag-and-drop |

### [`reflow.pack.api_services`](api_services/)

~6,700 generated wrappers for major SaaS APIs (Slack, Stripe, Jira,
Notion, Asana, GitHub, Discord, …). Template ids follow the pattern
`api_<service>_<operation>`. Each actor takes a JSON body or
form-encoded params on `input`, dispatches the request via `reqwest`
(rustls-tls), and emits the parsed response on `output`.

The full id set is too large to list here; enumerate post-load:

```js
require('@offbit-ai/reflow').loadPack(packPath);
console.log(require('@offbit-ai/reflow').templateList()
  .filter(id => id.startsWith('api_slack_')));
```

## Building from source

If you need to target a different rustc, hand-edit a manifest, or
prototype a custom pack, build any first-party pack from source:

```sh
# 1. Build the cdylib for your target triple. (Build for every triple
#    you want shipped; one entry per triple in Reflow.pack.toml.)
cargo build --release -p reflow_pack_ml --target aarch64-apple-darwin

# 2. Build the packaging CLI.
cargo build --release -p reflow_pack_cli

# 3. Read the host ABI version. The pack must be stamped with the
#    SAME number, or the loader will reject it.
target/release/reflow-pack abi
# abi_version = 1380148208
# host_triple = aarch64-apple-darwin

# 4. Bundle. The Reflow.pack.toml in each pack directory lists the
#    cdylib paths per triple.
REFLOW_PACK_ABI_VERSION=1380148208 target/release/reflow-pack build \
  --manifest sdk/packs/ml/Reflow.pack.toml \
  --out-dir target/packs

# 5. Inspect.
target/release/reflow-pack inspect target/packs/reflow.pack.ml-0.2.0.rflpack
```

## Authoring a third-party pack

1. New cdylib crate with one dep: `reflow_pack_sdk`.
2. Annotate a `fn(&mut PackHost)` with `#[reflow_pack]` — the macro
   emits the C ABI entrypoints.
3. Register your actor templates against an id of your choosing
   (collisions with first-party ids are rejected at load time).
4. Drop a `Reflow.pack.toml` next to `Cargo.toml`; ship a `.rflpack` via
   `reflow-pack build`.

```rust
use reflow_pack_sdk::{reflow_pack, Actor, PackHost};
use std::sync::Arc;

struct MyActor;
impl Actor for MyActor { /* … */ }

#[reflow_pack]
fn register(host: &mut PackHost) {
    host.register("my.pack.tick", || Arc::new(MyActor));
}
```

The result is a regular `.so` / `.dylib` / `.dll` you bundle into a
`.rflpack` and ship however you like — internal artifact registry,
GitHub Releases, npm tarball data file, etc. Any local file path
works with the SDKs' `loadPack()` / `load_pack()` / `LoadPack()` /
`Packs.loadPack()`.

## ABI lockstep

A pack's `reflow_pack_abi_version` symbol returns
`fnv1a(rustc_verbose_version || PACK_ABI_REVISION)`. The host computes
the same value at build time and refuses to dlopen a mismatched pack.
This means **a `.rflpack` is pinned to the rustc version of the SDK
release that produced it**.

| Releasing… | Use this `pack-v*` release |
|------------|------------------------------|
| `node-v0.2.0` | `pack-v0.2.0` |
| `python-v0.2.0` | `pack-v0.2.0` |

If you upgrade the SDK to a newer rustc, rebuild the pack from source
or wait for the corresponding `pack-v*` release.

## CI

`.github/workflows/publish-packs.yml` builds every pack listed above
on tag push (`pack-v*`), assembles one multi-triple `.rflpack` per
pack, and attaches them to a GitHub Release named `Packs <version>`.

Manual `workflow_dispatch` runs the build + bundle step without
publishing — useful for verifying a release branch before tagging.

## Why some features have no pack

| Feature | Reason |
|---------|--------|
| `av-core` | Behavior-only feature — same template ids exist regardless; bundled by default |
| `camera-native` | Behavior-only — `tpl_camera_capture` always exists; this feature swaps the backend (Nokhwa vs. mock) |
| `media` | Internal codec/types crate; no user-facing template ids |
