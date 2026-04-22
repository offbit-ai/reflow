# reflow_components — Backlog

Tracks outstanding work for the GPU / shader-graph / lighting / post-processing stack in `reflow_components`. Formerly `docs/shader_graph_plan.md`; converted to a backlog once the foundational phases landed.

Legend:
- ✅ shipped
- 🟡 partial (IR present but no actor / no codegen / limited use)
- ⬜ not started

---

## Foundation — shader graph IR + compilation ✅

- ✅ `reflow_shader` crate (`ir.rs`, `codegen.rs`) with `ShaderNode` IR and `compile()` → WGSL vertex + fragment pair.
- ✅ `CompiledMaterial` output with `vertex_wgsl`, `fragment_wgsl`, `texture_slots`, `material_uniforms`, `pipeline_hash`.
- ✅ `PbrUniforms` packed to std140-friendly layout.
- ✅ `compile_sdf_shade` — SDF shade-function injection used by SDF ray march path.

## Scene render integration ✅

- ✅ `SceneRenderActor` gained `material` inport and parses `Message::Object` `CompiledMaterial`.
- ✅ Dynamic pipeline cache keyed by `pipeline_hash` (`HashMap<u64, CachedDynamicPipeline>` under `OnceLock<Mutex<_>>`).
- ✅ Fallback paths retained for objects without a shader-graph material (vertex-color, textured).
- ⬜ LRU eviction on pipeline cache (currently unbounded — max 32 from plan not enforced).
- ⬜ `wgsl_debug` outport on `ShaderCompilerActor` for editor-side inspection.

## Shader node actors ✅

All 29 planned templates are registered:

`tpl_shader_const_float`, `tpl_shader_const_color`, `tpl_shader_texcoord`, `tpl_shader_position`, `tpl_shader_normal`, `tpl_shader_time`, `tpl_shader_vertex_color`, `tpl_shader_image_texture`, `tpl_shader_noise_texture`, `tpl_shader_checker_texture`, `tpl_shader_voronoi_texture`, `tpl_shader_gradient_texture`, `tpl_shader_brick_texture`, `tpl_shader_musgrave_texture`, `tpl_shader_wave_texture`, `tpl_shader_math`, `tpl_shader_color_mix`, `tpl_shader_color_ramp`, `tpl_shader_separate_xyz`, `tpl_shader_combine_xyz`, `tpl_shader_fresnel`, `tpl_shader_bump_map`, `tpl_shader_normal_map`, `tpl_shader_mapping`, `tpl_shader_clamp`, `tpl_shader_map_range`, `tpl_shader_principled_bsdf`, `tpl_shader_material_output`, `tpl_shader_compiler`.

Missing actors for IR variants that exist:
- ⬜ `tpl_shader_white_noise` (IR: `WhiteNoiseTexture`).
- ⬜ `tpl_shader_tangent` (IR: `Tangent`).
- ⬜ `tpl_shader_camera_vector` (IR: `CameraVector`).
- ⬜ `tpl_shader_environment_texture` (IR: `EnvironmentTexture`).
- ⬜ `tpl_shader_sky_texture` (IR: `SkyTexture`).
- ⬜ `tpl_shader_displacement` (IR: `Displacement`).

## PBR lighting ✅

- ✅ Cook–Torrance rasterized `pbr_shade` and `pbr_shade_multi` in WGSL.
- ✅ Multi-light loop: `u_scene.light_count` uniform, driven by `tpl_scene_light_collector`.
- ✅ SDF variant `pbr_shade_sdf` used by SDF ray march path.
- 🟡 Point / spot light handling — only directional is exercised end-to-end in examples. Need a targeted test.
- ⬜ Area lights (rectangular / disc) for studio-style lighting.

## Advanced BSDF — ArmorPaint parity

PrincipledBsdf IR carries every advanced parameter; codegen coverage is mixed.

| Feature | IR | Rasterized codegen | SDF codegen | Actor |
|---------|----|--------------------|-------------|-------|
| Clearcoat | ✅ | ✅ | ✅ | via `tpl_shader_principled_bsdf` |
| Sheen | ✅ | ✅ | ✅ | via `tpl_shader_principled_bsdf` |
| Subsurface | ✅ | 🟡 color/radius fields compiled, no scattering approximation in rasterized path | ✅ used by SDF | via `tpl_shader_principled_bsdf` |
| Transmission / IOR | ✅ | ⬜ ignored (`_` in destructure) | ✅ fully wired (refraction + dispersion in melting_ice) | via `tpl_shader_principled_bsdf` |
| Anisotropic | ✅ | ⬜ ignored in rasterized path | ⬜ | via `tpl_shader_principled_bsdf` |
| Iridescence / thin-film | ⬜ | — | — | — |

Pending:
- ⬜ Rasterized transmission (refraction through the backbuffer, chromatic dispersion toggle).
- ⬜ Anisotropic GGX in both rasterized and SDF paths.
- ⬜ Iridescence node.

## Lighting — environment / IBL

- ⬜ `EnvironmentTexture` + `SkyTexture` IR variants have no actor and no codegen path.
- ⬜ Prefiltered environment cubemap (specular split-sum).
- ⬜ Irradiance map (diffuse convolution).
- ⬜ BRDF LUT (split-sum integration).
- ⬜ Light probe actor for local reflections.

## Shadow mapping

- ✅ `tpl_shadow_map` CPU depth rasterizer actor.
- ⬜ Scene render samples the shadow map to modulate diffuse contribution (currently produces the map; nothing consumes it in the rasterized PBR path).
- ⬜ Directional cascaded shadow maps (CSM).
- ⬜ Point light cube maps.
- ⬜ Variance shadow maps / percentage-closer filtering.

## Post-processing

- ✅ `tpl_tone_map` (ACES filmic, Reinhard, Uncharted2 + gamma).
- ✅ `tpl_bloom` (threshold → downsample → separable blur → composite).
- ✅ `tpl_ssao` (screen-space AO from luminance approximation).
- ⬜ SSAO upgrade to HBAO/GTAO using real depth + normal buffers.
- ⬜ FXAA / TAA actor.
- ⬜ Depth-of-field / motion blur / chromatic aberration.
- ⬜ Post-process pipeline descriptor so a DAG can express a full chain without per-pair wiring.

## Displacement & surface tricks

- 🟡 `Displacement` IR variant exists; no actor, no codegen.
- ⬜ Vertex-stage displacement (height map sampling in vertex shader, auto-LOD friendly).
- ⬜ Parallax occlusion mapping in fragment shader.
- ⬜ Triplanar mapping (UV-free texturing for terrain / organic surfaces).
- ⬜ Decal projection (project texture onto surface via projector matrix).

## Bake actors

- ⬜ `tpl_bake_ao` — ray-traced ambient occlusion → texture.
- ⬜ `tpl_bake_curvature` — convexity / concavity map.
- ⬜ `tpl_bake_position` — world / object-space position map.
- ⬜ `tpl_bake_normal_transfer` — high-poly → low-poly normals.
- ⬜ `tpl_bake_thickness` — for SSS approximation.
- ⬜ All bake outputs stored in AssetDB as textures for reuse as shader inputs.

## Optimization

- ⬜ Shader module cache: hash `fragment_wgsl` → reuse `wgpu::ShaderModule` across pipelines that differ only in bind-group layout.
- ⬜ LRU eviction on `DYNAMIC_PIPELINE_CACHE` (cap around 32).
- ⬜ Benchmark the pipeline-creation hot path under a large material library.

## Docs / developer experience

- ⬜ Example graph JSON for each shader-node category in `docs/components/standard-library.md`.
- ⬜ Quick-start tutorial: "Author your first PBR material as a DAG".
- ⬜ Publish `reflow_shader` rustdoc to docs.rs (depends on the publishing work).

---

## Priority slate (next pass)

1. Fill the missing actors for existing IR variants (`WhiteNoiseTexture`, `Tangent`, `CameraVector`, `EnvironmentTexture`, `SkyTexture`, `Displacement`).
2. Wire the shadow map into the rasterized PBR path.
3. Rasterized transmission so the melting-ice quality generalises to mesh rendering.
4. Environment / IBL split-sum — the single biggest upgrade to surface realism.
5. LRU pipeline cache + shader-module dedupe before external users stress it.
