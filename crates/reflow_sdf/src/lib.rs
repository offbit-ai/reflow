//! SDF (Signed Distance Function) intermediate representation and WGSL code generation.
//!
//! This crate provides the building blocks for procedural geometry rendering
//! through Reflow's actor graph. Each actor contributes an [`SdfNode`] to a tree,
//! and the [`codegen`] module compiles the tree to a complete WGSL ray marching shader.
//!
//! # Architecture
//!
//! ```text
//! Actor Graph (Reflow)          SDF IR Tree              WGSL Shader
//! ┌──────────┐                  ┌─────────┐
//! │ Sphere   │──┐               │ Smooth  │              fn sdf(p: vec3f) -> f32 {
//! └──────────┘  ├─► Smooth ──►  │ Union   │  ──codegen──►  let d0 = sphere(p, 1.0);
//! ┌──────────┐  │   Union       │ ┌─┴──┐  │                let d1 = box(p, vec3(1));
//! │ Box      │──┘               │ S    B  │                return smin(d0, d1, 0.3);
//! └──────────┘                  └─────────┘              }
//! ```
//!
//! # Modules
//!
//! - [`ir`] — SDF node types (primitives, operations, transforms, materials)
//! - [`codegen`] — Compile SDF IR tree to WGSL shader source
//! - [`primitives`] — CPU-side SDF evaluation (for bounding, picking, tests)

pub mod codegen;
pub mod ir;
pub mod primitives;
