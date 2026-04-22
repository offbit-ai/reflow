# reflow_shader

Shader graph IR and WGSL material code generation for Reflow — Cook–Torrance PBR, procedural textures, color/math/effect nodes, and SDF shade compilation.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt)** which re-exports this crate as `reflow_rt::shader`. Direct use is appropriate when authoring custom material libraries or shader nodes outside the standard catalog.

## What it provides

- `ShaderNode` IR covering inputs, constants, math, color, textures, effects, and BSDFs (PrincipledBsdf with transmission, IOR, clearcoat, SSS, sheen, anisotropic).
- `codegen::compile` — rasterized vertex/fragment WGSL for mesh materials.
- `codegen::compile_sdf_shade_named` — injectable shade functions for SDF ray marching.
- Shared `PBR_FUNCTIONS` / `PBR_SDF_FUNCTIONS` WGSL blocks deduplicated across slots.

## Relationship to other crates

- Consumed by GPU shader nodes in `reflow_components` (compiler, principled, math, effects, textures, inputs).
- Pairs with `reflow_sdf` so a shader-graph material can shade a specific SDF material slot.

## License

MIT OR Apache-2.0.
