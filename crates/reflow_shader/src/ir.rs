//! Shader IR — intermediate representation for the node-based material graph.

use serde::{Deserialize, Serialize};

/// A node in the shader graph. Serialized as tagged JSON for DAG wiring.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ShaderNode {
    // ═══ Output ═══
    MaterialOutput {
        surface: Box<ShaderNode>,
    },

    // ═══ BSDF ═══
    PrincipledBsdf {
        base_color: Box<ShaderNode>,
        metallic: Box<ShaderNode>,
        roughness: Box<ShaderNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        normal: Option<Box<ShaderNode>>,
        emission: Box<ShaderNode>,
        emission_strength: Box<ShaderNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ao: Option<Box<ShaderNode>>,
        alpha: Box<ShaderNode>,
        // Extended PBR (ArmorPaint parity)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subsurface: Option<Box<ShaderNode>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subsurface_color: Option<Box<ShaderNode>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clearcoat: Option<Box<ShaderNode>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clearcoat_roughness: Option<Box<ShaderNode>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anisotropic: Option<Box<ShaderNode>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anisotropic_rotation: Option<Box<ShaderNode>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sheen: Option<Box<ShaderNode>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sheen_tint: Option<Box<ShaderNode>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transmission: Option<Box<ShaderNode>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ior: Option<Box<ShaderNode>>,
    },

    // ═══ Textures ═══
    ImageTexture {
        asset_id: String,
        uv: Box<ShaderNode>,
    },
    NoiseTexture {
        scale: Box<ShaderNode>,
        detail: Box<ShaderNode>,
        roughness: Box<ShaderNode>,
    },
    VoronoiTexture {
        scale: Box<ShaderNode>,
        randomness: Box<ShaderNode>,
    },
    CheckerTexture {
        scale: Box<ShaderNode>,
        color1: Box<ShaderNode>,
        color2: Box<ShaderNode>,
    },
    GradientTexture {
        gradient_type: GradientType,
    },
    BrickTexture {
        scale: Box<ShaderNode>,
        mortar_size: Box<ShaderNode>,
        color1: Box<ShaderNode>,
        color2: Box<ShaderNode>,
        mortar_color: Box<ShaderNode>,
    },
    MusgraveTexture {
        scale: Box<ShaderNode>,
        detail: Box<ShaderNode>,
        dimension: Box<ShaderNode>,
    },
    WaveTexture {
        wave_type: WaveType,
        scale: Box<ShaderNode>,
        distortion: Box<ShaderNode>,
    },
    WhiteNoiseTexture,

    // ═══ Environment ═══
    EnvironmentTexture {
        asset_id: String,
    },
    SkyTexture {
        sun_direction: Box<ShaderNode>,
        turbidity: Box<ShaderNode>,
    },

    // ═══ Vertex inputs ═══
    TexCoord,
    ObjectPosition,
    ObjectNormal,
    CameraVector,
    VertexColor,
    Tangent,
    Time,

    // ═══ Math ═══
    MathOp {
        op: MathOpType,
        a: Box<ShaderNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        b: Option<Box<ShaderNode>>,
    },
    ColorMix {
        mode: MixMode,
        fac: Box<ShaderNode>,
        a: Box<ShaderNode>,
        b: Box<ShaderNode>,
    },
    ColorRamp {
        stops: Vec<ColorStop>,
        input: Box<ShaderNode>,
    },
    SeparateXYZ {
        input: Box<ShaderNode>,
        component: String, // "x", "y", or "z"
    },
    CombineXYZ {
        x: Box<ShaderNode>,
        y: Box<ShaderNode>,
        z: Box<ShaderNode>,
    },
    Fresnel {
        ior: Box<ShaderNode>,
    },
    BumpMap {
        strength: Box<ShaderNode>,
        height: Box<ShaderNode>,
    },
    NormalMap {
        strength: Box<ShaderNode>,
        color: Box<ShaderNode>,
    },
    Mapping {
        location: [f32; 3],
        rotation: [f32; 3],
        scale: [f32; 3],
        input: Box<ShaderNode>,
    },
    Clamp {
        input: Box<ShaderNode>,
        min_val: f32,
        max_val: f32,
    },
    MapRange {
        input: Box<ShaderNode>,
        from_min: f32,
        from_max: f32,
        to_min: f32,
        to_max: f32,
    },

    // ═══ Displacement ═══
    Displacement {
        height: Box<ShaderNode>,
        midlevel: Box<ShaderNode>,
        scale: Box<ShaderNode>,
    },

    // ═══ Constants ═══
    ConstFloat(f32),
    ConstVec3([f32; 3]),
    ConstVec4([f32; 4]),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MathOpType {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
    Sqrt,
    Abs,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan2,
    Min,
    Max,
    Floor,
    Ceil,
    Fract,
    Modulo,
    Smoothstep,
    Lerp,
    Step,
    Sign,
    Log,
    Exp,
    Dot,
    Cross,
    Normalize,
    Length,
    Distance,
    Reflect,
    Negate,
    Invert, // 1 - x
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MixMode {
    Mix,
    Add,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Dodge,
    Burn,
    SoftLight,
    LinearLight,
    Difference,
    Subtract,
    Divide,
    Hue,
    Saturation,
    Color,
    Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GradientType {
    Linear,
    Quadratic,
    Easing,
    Diagonal,
    Spherical,
    Radial,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WaveType {
    Bands,
    Rings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorStop {
    pub position: f32,
    pub color: [f32; 4],
}

/// Texture slot required by a compiled material.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureSlot {
    pub binding: u32,
    pub asset_id: String,
    pub label: String,
}

/// Vertex attribute descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VertexAttr {
    pub name: String,
    pub format: String,
    pub offset: u32,
    pub shader_location: u32,
}

/// Compiled material — output of the shader graph codegen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledMaterial {
    pub vertex_wgsl: String,
    pub fragment_wgsl: String,
    pub vertex_stride: u32,
    pub vertex_attributes: Vec<VertexAttr>,
    pub texture_slots: Vec<TextureSlot>,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emission_color: [f32; 3],
    pub emission_strength: f32,
    pub ao_strength: f32,
    pub pipeline_hash: u64,
}
