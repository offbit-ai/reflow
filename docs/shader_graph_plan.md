# Node-Based Shader Graph System — Implementation Plan

## 1. Architecture Overview

### Design: Follow the SDF IR + Codegen Pattern

The existing `reflow_sdf` crate establishes a proven pattern:

1. **Actors produce IR nodes** as `Message::Object` (serialized JSON)
2. **A compiler actor** collects the IR tree and calls a codegen module
3. **The codegen module** walks the tree and emits WGSL source code
4. **A render actor** receives compiled WGSL + metadata and creates GPU pipelines

Each shader node actor emits a `ShaderNode` IR object. A `ShaderCompilerActor` receives the root node, walks the graph, and emits WGSL fragment shader code. The compiled material (WGSL string + bind group layout + vertex format) is sent to `SceneRenderActor` via a new `material` inport.

### Shader IR Format (`ShaderNode` enum)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ShaderNode {
    // Output node — terminal
    MaterialOutput { surface: Box<ShaderNode> },

    // Shader (BSDF) nodes
    PrincipledBsdf {
        base_color: Box<ShaderNode>,          // vec3f / vec4f
        metallic: Box<ShaderNode>,            // f32
        roughness: Box<ShaderNode>,           // f32
        normal: Option<Box<ShaderNode>>,      // vec3f (tangent space)
        emission: Box<ShaderNode>,            // vec3f
        emission_strength: Box<ShaderNode>,   // f32
        ao: Option<Box<ShaderNode>>,          // f32
        alpha: Box<ShaderNode>,               // f32
    },

    // Texture nodes
    ImageTexture { asset_id: String, uv: Box<ShaderNode> },
    NoiseTexture { scale: Box<ShaderNode>, detail: Box<ShaderNode>, roughness: Box<ShaderNode> },
    VoronoiTexture { scale: Box<ShaderNode>, randomness: Box<ShaderNode> },
    CheckerTexture { scale: Box<ShaderNode>, color1: Box<ShaderNode>, color2: Box<ShaderNode> },
    GradientTexture { gradient_type: GradientType },
    BrickTexture { /* mortar, scale, etc. */ },

    // Input nodes (read from vertex data or uniforms)
    TexCoord,         // outputs vec2f UV
    ObjectPosition,   // world position vec3f
    ObjectNormal,     // world normal vec3f
    CameraVector,     // view direction vec3f
    VertexColor,      // vertex color vec3f
    Time,             // animation time f32

    // Math / utility
    MathOp { op: MathOpType, a: Box<ShaderNode>, b: Option<Box<ShaderNode>> },
    ColorMix { mode: MixMode, fac: Box<ShaderNode>, a: Box<ShaderNode>, b: Box<ShaderNode> },
    ColorRamp { stops: Vec<(f32, [f32; 4])>, input: Box<ShaderNode> },
    SeparateXYZ { input: Box<ShaderNode> },
    CombineXYZ { x: Box<ShaderNode>, y: Box<ShaderNode>, z: Box<ShaderNode> },
    Fresnel { ior: Box<ShaderNode> },
    BumpMap { strength: Box<ShaderNode>, height: Box<ShaderNode> },
    NormalMap { strength: Box<ShaderNode>, color: Box<ShaderNode> },
    Mapping { location: [f32;3], rotation: [f32;3], scale: [f32;3], input: Box<ShaderNode> },

    // Advanced BSDF (ArmorPaint parity)
    SubsurfaceScatter { color: Box<ShaderNode>, radius: Box<ShaderNode>, scale: Box<ShaderNode> },
    Clearcoat { roughness: Box<ShaderNode>, normal: Option<Box<ShaderNode>> },
    Anisotropic { anisotropy: Box<ShaderNode>, rotation: Box<ShaderNode> },
    Sheen { color: Box<ShaderNode>, roughness: Box<ShaderNode> },
    Transmission { ior: Box<ShaderNode>, roughness: Box<ShaderNode> },

    // Environment
    EnvironmentTexture { asset_id: String },
    SkyTexture { sun_direction: Box<ShaderNode>, turbidity: Box<ShaderNode> },

    // Displacement
    Displacement { height: Box<ShaderNode>, midlevel: Box<ShaderNode>, scale: Box<ShaderNode> },
    VectorDisplacement { vector: Box<ShaderNode>, midlevel: Box<ShaderNode>, scale: Box<ShaderNode> },

    // Advanced utility
    Clamp { input: Box<ShaderNode>, min: Box<ShaderNode>, max: Box<ShaderNode> },
    MapRange { input: Box<ShaderNode>, from_min: f32, from_max: f32, to_min: f32, to_max: f32 },
    Wave { wave_type: WaveType, scale: Box<ShaderNode>, distortion: Box<ShaderNode> },
    MusgraveTexture { scale: Box<ShaderNode>, detail: Box<ShaderNode>, dimension: Box<ShaderNode> },
    WhiteNoiseTexture { dimensions: u32 },
    Tangent,
    Geometry,  // outputs multiple: position, normal, tangent, parametric, etc.

    // Constants (leaves)
    ConstFloat(f32),
    ConstVec3([f32; 3]),
    ConstVec4([f32; 4]),
}
```

### Compilation Flow

```
                DAG Actor Graph
  ┌───────────┐    ┌──────────────┐    ┌──────────────┐
  │NoiseTexture│──►│  ColorMix    │──►│PrincipledBSDF│──►┐
  └───────────┘    └──────────────┘    └──────────────┘  │
  ┌───────────┐         ▲                                │
  │ConstFloat │─────────┘                                │
  └───────────┘                                          │
                                              ┌──────────▼──────────┐
                                              │ShaderCompilerActor  │
                                              │  - walks IR tree    │
                                              │  - emits WGSL frag  │
                                              │  - determines BGL   │
                                              └──────────┬──────────┘
                                                         │
                                          Message::Object(CompiledMaterial)
                                                 {wgsl, vertex_format,
                                                  texture_slots, uniforms}
                                                         │
                                              ┌──────────▼──────────┐
                                              │ SceneRenderActor    │
                                              │  (new `material`    │
                                              │   inport)           │
                                              └─────────────────────┘
```

### Key Architecture Decisions

**Q1: IR vs static config?**
IR approach. Each shader node actor outputs a `ShaderNode` subtree. The `PrincipledBsdfActor` collects subtrees from its inports and nests them. `ShaderCompilerActor` receives the final `MaterialOutput` tree and generates WGSL. Matches the SDF pattern.

**Q2: How does compiled shader reach the GPU?**
Via a `material` inport on `SceneRenderActor` carrying `Message::Object`:
```json
{
  "fragmentWgsl": "...",
  "vertexFormat": "pbr44",
  "textureSlots": [
    {"binding": 1, "assetId": "tex_abc", "type": "2d"}
  ],
  "materialUniforms": {
    "baseColor": [0.8, 0.1, 0.1],
    "metallic": 0.0,
    "roughness": 0.5
  },
  "pipelineHash": "sha256..."
}
```

**Q3: Pipeline caching?**
Replace `OnceLock` statics with `HashMap<u64, CachedScenePipeline>` keyed by hash of (vertex format + WGSL + sample count + texture layout). `parking_lot::RwLock` around it.

---

## 2. New Crate: `reflow_shader`

Following `reflow_sdf`, create `crates/reflow_shader/`:

- `src/ir.rs` — `ShaderNode` enum, `CompiledMaterial`, `PbrMaterialUniforms`
- `src/codegen.rs` — Walk `ShaderNode` tree, emit WGSL fragment + vertex shader pair
- `src/lib.rs` — Re-exports

### `CompiledMaterial` Output

```rust
pub struct CompiledMaterial {
    pub vertex_wgsl: String,
    pub fragment_wgsl: String,
    pub vertex_stride: u32,
    pub vertex_attributes: Vec<VertexAttr>,
    pub texture_slots: Vec<TextureSlot>,
    pub material_uniforms: PbrUniforms,
    pub pipeline_hash: u64,
}
```

### `PbrUniforms`

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PbrUniforms {
    pub base_color: [f32; 4],       // 16
    pub metallic: f32,              // 4
    pub roughness: f32,             // 4
    pub emission_strength: f32,     // 4
    pub ao_strength: f32,           // 4  → 32
    pub emission_color: [f32; 3],   // 12
    pub _pad0: f32,                 // 4  → 48
}
```

---

## 3. Shader Node Actors

### Source Nodes (no shader inports)

| Actor | Template ID | Output |
|-------|------------|--------|
| `ShaderConstFloatActor` | `tpl_shader_const_float` | `ConstFloat` |
| `ShaderConstColorActor` | `tpl_shader_const_color` | `ConstVec3` |
| `ShaderTexCoordActor` | `tpl_shader_texcoord` | `TexCoord` |
| `ShaderPositionActor` | `tpl_shader_position` | `ObjectPosition` |
| `ShaderNormalActor` | `tpl_shader_normal` | `ObjectNormal` |
| `ShaderTimeActor` | `tpl_shader_time` | `Time` |
| `ShaderVertexColorActor` | `tpl_shader_vertex_color` | `VertexColor` |

### Texture Nodes

| Actor | Template ID | Inports |
|-------|------------|---------|
| `ShaderImageTextureActor` | `tpl_shader_image_texture` | `uv` |
| `ShaderNoiseTextureActor` | `tpl_shader_noise_texture` | `scale`, `detail`, `roughness` |
| `ShaderCheckerTextureActor` | `tpl_shader_checker_texture` | `scale`, `color1`, `color2` |
| `ShaderVoronoiTextureActor` | `tpl_shader_voronoi_texture` | `scale`, `randomness` |
| `ShaderGradientTextureActor` | `tpl_shader_gradient_texture` | — |
| `ShaderBrickTextureActor` | `tpl_shader_brick_texture` | `scale`, `mortar_size` |

### Math / Utility Nodes

| Actor | Template ID | Inports |
|-------|------------|---------|
| `ShaderMathActor` | `tpl_shader_math` | `a`, `b` |
| `ShaderColorMixActor` | `tpl_shader_color_mix` | `fac`, `a`, `b` |
| `ShaderColorRampActor` | `tpl_shader_color_ramp` | `input` |
| `ShaderSeparateXYZActor` | `tpl_shader_separate_xyz` | `input` |
| `ShaderCombineXYZActor` | `tpl_shader_combine_xyz` | `x`, `y`, `z` |
| `ShaderFresnelActor` | `tpl_shader_fresnel` | `ior` |
| `ShaderBumpMapActor` | `tpl_shader_bump_map` | `strength`, `height` |
| `ShaderNormalMapActor` | `tpl_shader_normal_map` | `strength`, `color` |
| `ShaderMappingActor` | `tpl_shader_mapping` | `input` |

### BSDF / Output Nodes

| Actor | Template ID | Inports |
|-------|------------|---------|
| `ShaderPrincipledBsdfActor` | `tpl_shader_principled_bsdf` | `base_color`, `metallic`, `roughness`, `normal`, `emission`, `emission_strength`, `ao`, `alpha` |
| `ShaderMaterialOutputActor` | `tpl_shader_material_output` | `surface` |
| `ShaderCompilerActor` | `tpl_shader_compiler` | `shader` |

---

## 4. PBR Lighting Model (Cook-Torrance)

Generated WGSL fragment shader uses:

```wgsl
// GGX/Trowbridge-Reitz Normal Distribution Function
fn D_GGX(NoH: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = (NoH * NoH) * (a2 - 1.0) + 1.0;
    return a2 / (3.14159 * d * d);
}

// Schlick-GGX Geometry Function
fn G_SchlickGGX(NdotV: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return NdotV / (NdotV * (1.0 - k) + k);
}

fn G_Smith(NoV: f32, NoL: f32, roughness: f32) -> f32 {
    return G_SchlickGGX(NoV, roughness) * G_SchlickGGX(NoL, roughness);
}

// Schlick Fresnel Approximation
fn F_Schlick(cosTheta: f32, F0: vec3f) -> vec3f {
    return F0 + (1.0 - F0) * pow(1.0 - cosTheta, 5.0);
}

fn pbr_shade(
    base_color: vec3f, metallic: f32, roughness: f32,
    N: vec3f, V: vec3f, L: vec3f, light_color: vec3f,
    ao: f32, emission: vec3f
) -> vec3f {
    let H = normalize(V + L);
    let NoL = max(dot(N, L), 0.0);
    let NoV = max(dot(N, V), 0.001);
    let NoH = max(dot(N, H), 0.0);
    let HoV = max(dot(H, V), 0.0);

    let F0 = mix(vec3f(0.04), base_color, metallic);

    let D = D_GGX(NoH, roughness);
    let G = G_Smith(NoV, NoL, roughness);
    let F = F_Schlick(HoV, F0);

    // Specular
    let spec = (D * G * F) / max(4.0 * NoV * NoL, 0.001);

    // Diffuse (energy-conserving Lambert)
    let kD = (vec3f(1.0) - F) * (1.0 - metallic);
    let diffuse = kD * base_color / 3.14159;

    let radiance = light_color * NoL;
    let color = (diffuse + spec) * radiance;
    let ambient = vec3f(0.03) * base_color * ao;

    return ambient + color + emission;
}
```

---

## 5. Changes to `scene_render.rs`

### New Inport
```rust
#[actor(SceneRenderActor, inports::<10>(scene, meshes, terrain_mesh, texture, material), ...)]
```

### Dynamic Pipeline Cache
Replace 4 static `OnceLock` pipelines with keyed cache:
```rust
static PIPELINE_CACHE: Lazy<Mutex<HashMap<u64, CachedScenePipeline>>> = ...;
```

### Material-Aware Rendering
When `material` present on inport:
1. Parse `CompiledMaterial` from `Message::Object`
2. Check `PIPELINE_CACHE` by `pipeline_hash`
3. If miss: create `ShaderModule` from WGSL, build `BindGroupLayout`, create `RenderPipeline`
4. Build bind group: scene uniforms (binding 0) + material uniforms (binding 1) + textures (binding 2+)
5. Render with dynamic pipeline

When no `material`: fall back to existing hardcoded paths (backward compatible).

---

## 6. Implementation Phases

### Phase 1: Foundation — `reflow_shader` crate + constant PBR materials
- Create `crates/reflow_shader/` with IR + codegen
- Define `ShaderNode` enum (ConstFloat, ConstVec3, PrincipledBsdf, MaterialOutput, TexCoord, ObjectPosition, ObjectNormal)
- Implement `codegen::compile()` — walk tree, emit WGSL with Cook-Torrance PBR
- Add `material` inport to SceneRenderActor, implement dynamic pipeline cache
- Create 3 actors: `ShaderPrincipledBsdfActor`, `ShaderMaterialOutputActor`, `ShaderCompilerActor`
- End-to-end test: constant-color PBR renders with specular highlights

### Phase 2: Texture support
- Add `ImageTexture`, `NoiseTexture`, `CheckerTexture` to IR
- Extend codegen for `textureSample()` calls + texture binding slots
- Create texture actors + `ShaderTexCoordActor`
- Implement WGSL noise functions (port from `reflow_sdf/src/noise.rs`)

### Phase 3: Math and mixing nodes
- Add `MathOp`, `ColorMix`, `ColorRamp`, `Fresnel`, `SeparateXYZ`, `CombineXYZ`
- Create corresponding actors
- Test complex graphs: Fresnel → mix factor → base_color

### Phase 4: Normal mapping and advanced features
- Add `NormalMap`, `BumpMap`, `Mapping` to IR
- Extend vertex shader for tangent+bitangent
- 44-byte vertex format (pos3+normal3+uv2+tangent4)
- Add `VoronoiTexture`, `GradientTexture`, `BrickTexture`

### Phase 5: Multi-material and optimization
- Per-object material binding in scene graph
- LRU pipeline cache eviction (max 32)
- Shader compilation caching (hash WGSL → reuse ShaderModule)
- `wgsl_debug` outport for shader inspector

### Phase 6: Advanced BSDF — ArmorPaint parity
- **Subsurface scattering** node (SSS approximation for skin, wax, leaves)
- **Clearcoat** layer (secondary specular lobe for car paint, lacquer)
- **Anisotropic** shading (brushed metal, hair — anisotropic GGX)
- **Sheen** (fabric, velvet)
- **Transmission** (glass, thin-film transparency)
- Extend PrincipledBSDF with all parameters matching Blender/ArmorPaint

### Phase 7: Lighting system
- **Multi-light support** (point, directional, spot — uniform array)
- **Shadow mapping** (directional cascade shadow maps, point light cube maps)
- **Environment/HDRI lighting** — IBL with split-sum approximation:
  - Prefiltered environment cubemap for specular
  - Irradiance map for diffuse
  - BRDF LUT for split-sum integration
- **Light probe** actors for local reflections

### Phase 8: Screen-space post-processing
- **SSAO** (screen-space ambient occlusion — HBAO or GTAO)
- **Bloom** (threshold + Gaussian blur + additive composite)
- **Tone mapping** (ACES filmic, Reinhard, exposure control)
- **Anti-aliasing** (FXAA/TAA post-process)
- Post-process pipeline as a chain of compute/fragment passes

### Phase 9: Bake nodes
- **AO bake** — ray-traced ambient occlusion to texture
- **Curvature bake** — convexity/concavity map
- **Position bake** — world/object space position to texture
- **Normal bake** — high-poly to low-poly normal transfer
- **Thickness bake** — for SSS approximation
- All bake results stored in AssetDB as textures for material input

### Phase 10: Displacement and decals
- **Vertex displacement** — height map → vertex offset in vertex shader
- **Parallax occlusion mapping** — fragment-level displacement approximation
- **Decal projection** — project textures onto mesh surfaces via projector matrix
- **Triplanar mapping** — UV-free texturing for terrain/organic surfaces

---

## 7. File Organization

### New files
```
crates/reflow_shader/
  Cargo.toml
  src/
    lib.rs
    ir.rs          — ShaderNode, CompiledMaterial, PbrUniforms
    codegen.rs     — compile(), CodegenContext, WGSL templates

crates/reflow_components/src/gpu/shader/
  mod.rs           — module declarations
  compiler.rs      — ShaderCompilerActor
  principled.rs    — ShaderPrincipledBsdfActor, ShaderMaterialOutputActor
  textures.rs      — ImageTexture, NoiseTexture, CheckerTexture, etc.
  inputs.rs        — TexCoord, Position, Normal, Time, VertexColor
  math.rs          — MathOp, ColorMix, ColorRamp, SeparateXYZ, etc.
  effects.rs       — Fresnel, NormalMap, BumpMap, Mapping
```

### Modified files
```
crates/reflow_components/src/gpu/mod.rs           — add `pub mod shader;`
crates/reflow_components/src/gpu/scene_render.rs  — material inport, dynamic pipeline
crates/reflow_components/src/registry.rs          — register shader actors
crates/reflow_components/Cargo.toml               — add reflow_shader dependency
```
