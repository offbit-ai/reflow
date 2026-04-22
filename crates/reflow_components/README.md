# reflow_components

The standard Reflow component catalog — procedural, media, vector, SDF/GPU, animation, physics, scene, I/O, text, stream ops, logic, and system actors.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt)**, which re-exports this crate as `reflow_rt::components` and owns the feature gates. Direct use is appropriate when you want the catalog without the `reflow_rt` facade layer.

## What it provides

- `get_actor_for_template(&str) -> Option<Arc<dyn Actor>>` — the template-id-based actor factory.
- `get_template_mapping()` / editor metadata for discovery.
- **274** first-party actor templates (`tpl_*`) across animation, GPU rendering (scene render, SDF ray march, shader graph, PBR), stream ops (video encode, image decode, audio DSP), procedural (noise, voronoi, mesh, vertex color), I/O (glTF, OBJ, STL, FBX), flow control, scene / ECS, and more.

## Complete template catalog

Every `tpl_*` template this crate registers, grouped by category, is documented here:

**→ [Standard Component Library catalog](https://github.com/offbit-ai/reflow/blob/main/docs/components/standard-library.md)**

### Per feature-gate references

| Feature | Reference |
|---------|-----------|
| `gpu` | [GPU actors](https://github.com/offbit-ai/reflow/blob/main/docs/components/gpu-actors.md) — SDF ray march, scene render, 2D render, shader graph, post-processing. |
| `av-core` | [Audio / DSP actors](https://github.com/offbit-ai/reflow/blob/main/docs/components/audio-actors.md) — filters, dynamics, spectral, time/pitch. |
| `window-events` | [Input / window event actors](https://github.com/offbit-ai/reflow/blob/main/docs/components/input-actors.md) — keyboard, mouse, gamepad, touch, window. |
| `browser` / `browser-events` | [Browser actors](https://github.com/offbit-ai/reflow/blob/main/docs/components/browser-actors.md) — headless screencast and browser DOM input events. |
| `ml` | [Media / ML stack](https://github.com/offbit-ai/reflow/blob/main/docs/components/ml-stack.md) — inference boundary, preprocess, decode, taskpacks. |
| media / `camera-native` / `video-encode` | [Media actors](https://github.com/offbit-ai/reflow/blob/main/docs/components/media-actors.md) — image / audio / video input, camera capture, video encode. |
| `api_services` | [API services catalog](https://github.com/offbit-ai/reflow/blob/main/docs/components/api-actors.md) — ~6,700 `api_*` templates (lives in `reflow_api_services`). |

## Feature gates

The default surface is `av-core + gpu + window-events`. Opt in to the rest:

| Feature | Adds |
| --- | --- |
| `av-core` | Audio / signal actors through `reflow_dsp` |
| `gpu` | wgpu-backed 2D / 3D / SDF / shader actors |
| `window-events` | Window and input actors |
| `browser-events` / `browser` | Browser automation |
| `camera-native` | Native camera capture |
| `video-encode` | Native video encoder |
| `ml` | CV preprocess + inference + decode + taskpacks |
| `api_services` | Large API-services actor catalog |

## License

MIT OR Apache-2.0.
