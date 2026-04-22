# GPU Actors

Actors behind the `gpu` feature of `reflow_components` / `reflow_rt`. Enabling `gpu` pulls in `wgpu`, `pollster`, and `once_cell`, and unlocks the compute and render actors listed below.

```toml
reflow_rt = { version = "0.1", features = ["gpu"] }
```

## What's included

| Category | Templates | Notes |
|----------|-----------|-------|
| SDF ray march / mesh | `tpl_sdf_live_render`, `tpl_sdf_render`, `tpl_sdf_marching_cubes`, `tpl_mesh_to_sdf` | Consumes SDF IR built by `tpl_sdf_*` primitive/op actors (always available). |
| Scene rendering | `tpl_scene_render` | Rasterizer for triangle meshes with PBR. Accepts compiled shader-graph materials. |
| 2D rendering | `tpl_gpu_2d_render` | Canvas-style 2D composited output (shapes, glyphs, images). |
| Shader graph | `tpl_shader_*` (compiler, principled, math, effects, textures, inputs) | Produces WGSL consumed by `tpl_scene_render` and `tpl_sdf_scene`. |
| Post-processing | `tpl_tone_map`, `tpl_bloom`, `tpl_ssao`, `tpl_shadow_map` | Applied downstream of scene/SDF render. |

SDF IR composition (`tpl_sdf_sphere`, `tpl_sdf_box`, `tpl_sdf_union`, `tpl_sdf_path`, etc.) is **always available** — only the final rasterizer / ray marcher is GPU-gated.

## Requirements

- A wgpu-compatible backend on the target platform.
- On macOS and iOS: Metal. On Windows: DX12 or Vulkan. On Linux: Vulkan.
- In browsers (Wasm): WebGPU. Behavior of native-only actors is scoped out.

## Complete per-template catalog

See **[standard-library.md § GPU, SDF, Shader Graph, Post-Processing sections](./standard-library.md)**.

## Related

- [Media actors](./media-actors.md) — including `tpl_video_encoder` (`video-encode` feature) which commonly consumes GPU render output.
- [ML stack](./ml-stack.md) — uses the same `reflow_media_types` packets that scene-render and SDF actors produce.
