//! Reflow Shader Graph — node-based material system with WGSL codegen.
//!
//! Follows the SDF IR + codegen pattern:
//! 1. DAG actors produce `ShaderNode` IR trees
//! 2. `codegen::compile()` walks the tree and emits WGSL shaders
//! 3. Scene render creates GPU pipelines from compiled materials

pub mod codegen;
pub mod ir;

pub use codegen::compile;
pub use ir::{CompiledMaterial, MathOpType, MixMode, ShaderNode};
