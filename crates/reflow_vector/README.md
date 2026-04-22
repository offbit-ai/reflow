# reflow_vector

2D vector graphics primitives used by Reflow vector actors — paths, shapes, boolean operations, rasterization.

> **Most users should depend on [`reflow_rt`](https://docs.rs/reflow_rt)** which re-exports this crate as `reflow_rt::vector`. Direct use is appropriate when implementing custom shape or stroke processing outside the component catalog.

## What it provides

- `Path`, `Shape`, fill/stroke parameters.
- Boolean path operations (union, intersect, subtract).
- Rasterization helpers used by the GPU 2D renderer and SDF-text actors.

## Relationship to other crates

- Consumed by vector- and text-related actors in `reflow_components` (canvas, background, rasterize, shape, blur, blend).
- Pairs with `reflow_sdf` for 2D/3D procedural shapes.

## License

MIT OR Apache-2.0.
