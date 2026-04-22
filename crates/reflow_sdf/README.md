# reflow_sdf

Signed Distance Field IR, primitives, operations, and WGSL code generation for Reflow's procedural rendering actors.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt)** which re-exports this crate as `reflow_rt::sdf`. Direct use is appropriate when writing custom procedural-geometry actors or offline SDF tools.

## What it provides

- `SdfNode` IR: primitives (sphere, box, cylinder, torus, tube path, puddle, …), CSG operations (union, intersect, subtract, smooth variants), transforms, material slots, scene settings.
- `codegen::compile` — emits a ready-to-use WGSL compute shader for ray marching.
- Slot-dispatch support: material shaders from `reflow_shader` can plug into specific named regions of an SDF scene.

## Quick glance

```rust
use reflow_sdf::{ir::*, codegen::compile};

let scene = SdfNode::Scene {
    root: Box::new(SdfNode::Primitive {
        prim: SdfPrimitive::Sphere { radius: 1.0 },
        material: None,
    }),
    settings: SceneSettings::default(),
};

let compiled = compile(&scene);
println!("{}", compiled.wgsl);
```

In a Reflow graph, the SDF tree is authored by `tpl_sdf_*` actors, then consumed by `tpl_sdf_render` (ray march) or `tpl_sdf_marching_cubes` (mesh).

## License

MIT OR Apache-2.0.
