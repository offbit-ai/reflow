//! SDF Intermediate Representation.
//!
//! The IR is a tree of [`SdfNode`] values. Each node represents either a
//! primitive shape, a CSG operation combining children, a domain transform,
//! or a material assignment. Actors serialize/deserialize these as JSON via
//! `Message::Object`.

use serde::{Deserialize, Serialize};

// ─── Primitives ──────────────────────────────────────────────────────

/// A primitive SDF shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "primitive", rename_all = "camelCase")]
pub enum SdfPrimitive {
    Sphere {
        radius: f32,
    },
    Box {
        size: [f32; 3],
    },
    RoundBox {
        size: [f32; 3],
        radius: f32,
    },
    Cylinder {
        radius: f32,
        height: f32,
    },
    Capsule {
        radius: f32,
        height: f32,
    },
    Torus {
        major_radius: f32,
        minor_radius: f32,
    },
    Cone {
        angle: f32,
        height: f32,
    },
    Plane {
        normal: [f32; 3],
        offset: f32,
    },
    /// Line segment with linearly varying radius (conical frustum with rounded ends).
    /// `a` and `b` are endpoints, `radius_a` and `radius_b` are the radii at each end.
    TaperedCapsule {
        a: [f32; 3],
        b: [f32; 3],
        radius_a: f32,
        radius_b: f32,
    },
    /// Complete tubular path — evaluates smooth minimum distance across all
    /// segments in a single SDF call. No union artifacts between segments.
    /// Points are 3D positions, radii are per-point cross-section radii.
    /// `k` is the smooth min blending factor (0 = hard min).
    TubePath {
        points: Vec<[f32; 3]>,
        radii: Vec<f32>,
        k: f32,
    },
    /// Infinite repetition — creates `child` at every grid cell.
    InfRepeat {
        spacing: [f32; 3],
    },
}

// ─── Operations ──────────────────────────────────────────────────────

/// CSG (Constructive Solid Geometry) operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum SdfOp {
    /// Hard boolean union (min).
    Union,
    /// Hard boolean intersection (max).
    Intersection,
    /// Hard boolean difference (max of a, -b).
    Difference,
    /// Smooth union with blending radius `k`.
    SmoothUnion { k: f32 },
    /// Smooth intersection with blending radius `k`.
    SmoothIntersection { k: f32 },
    /// Smooth difference with blending radius `k`.
    SmoothDifference { k: f32 },
}

// ─── Transforms ──────────────────────────────────────────────────────

/// Domain transforms applied before evaluating the child SDF.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "transform", rename_all = "camelCase")]
pub enum SdfTransform {
    /// Translate the SDF in world space.
    Translate { offset: [f32; 3] },
    /// Rotate by Euler angles (degrees) around X, Y, Z.
    Rotate { angles: [f32; 3] },
    /// Uniform or non-uniform scale.
    Scale { factor: [f32; 3] },
    /// Twist around Y axis by `strength` radians per unit.
    Twist { strength: f32 },
    /// Bend around Y axis by `strength` radians per unit along X.
    Bend { strength: f32 },
    /// Elongate along axes before SDF evaluation.
    Elongate { amount: [f32; 3] },
    /// Round edges by subtracting `radius` from the SDF.
    Round { radius: f32 },
    /// Shell — make the shape hollow with wall `thickness`.
    Shell { thickness: f32 },
    /// Finite repetition in a bounding region.
    Repeat { spacing: [f32; 3], count: [u32; 3] },
    /// Mirror across a plane.
    Mirror { axis: [f32; 3] },
    /// Displacement — add noise or function to the SDF value.
    Displace {
        frequency: f32,
        amplitude: f32,
        octaves: u32,
    },
}

// ─── Materials ───────────────────────────────────────────────────────

/// Surface material properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdfMaterial {
    /// Base color (linear RGB, 0–1).
    #[serde(default = "default_color")]
    pub color: [f32; 3],
    /// Roughness (0 = mirror, 1 = matte).
    #[serde(default = "default_roughness")]
    pub roughness: f32,
    /// Metallic factor (0 = dielectric, 1 = metal).
    #[serde(default)]
    pub metallic: f32,
    /// Emission color and intensity.
    #[serde(default)]
    pub emission: [f32; 3],
    /// Index of refraction for Fresnel (default 1.5 = glass).
    #[serde(default = "default_ior")]
    pub ior: f32,
}

fn default_color() -> [f32; 3] {
    [0.8, 0.8, 0.8]
}
fn default_roughness() -> f32 {
    0.5
}
fn default_ior() -> f32 {
    1.5
}

impl Default for SdfMaterial {
    fn default() -> Self {
        Self {
            color: default_color(),
            roughness: default_roughness(),
            metallic: 0.0,
            emission: [0.0; 3],
            ior: default_ior(),
        }
    }
}

// ─── Scene Settings ──────────────────────────────────────────────────

/// Camera and render settings for the scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneSettings {
    /// Camera position in world space.
    #[serde(default = "default_camera_pos")]
    pub camera_pos: [f32; 3],
    /// Camera look-at target.
    #[serde(default)]
    pub camera_target: [f32; 3],
    /// Vertical field of view in degrees.
    #[serde(default = "default_fov")]
    pub fov: f32,
    /// Maximum ray march steps.
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
    /// Maximum ray distance.
    #[serde(default = "default_max_dist")]
    pub max_dist: f32,
    /// Surface hit threshold.
    #[serde(default = "default_epsilon")]
    pub epsilon: f32,
    /// Background color.
    #[serde(default = "default_bg")]
    pub background: [f32; 3],
    /// Ambient light intensity.
    #[serde(default = "default_ambient")]
    pub ambient: f32,
    /// Primary directional light direction (normalized).
    #[serde(default = "default_light_dir")]
    pub light_dir: [f32; 3],
    /// Light color.
    #[serde(default = "default_light_color")]
    pub light_color: [f32; 3],
    /// Enable soft shadows.
    #[serde(default)]
    pub soft_shadows: bool,
    /// Shadow softness factor.
    #[serde(default = "default_shadow_k")]
    pub shadow_k: f32,
    /// Enable ambient occlusion.
    #[serde(default = "default_true")]
    pub ao: bool,
    /// Output resolution.
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    /// Animation time (seconds) — passed to shader as uniform.
    #[serde(default)]
    pub time: f32,
}

fn default_camera_pos() -> [f32; 3] {
    [3.0, 2.0, 4.0]
}
fn default_fov() -> f32 {
    45.0
}
fn default_max_steps() -> u32 {
    128
}
fn default_max_dist() -> f32 {
    100.0
}
fn default_epsilon() -> f32 {
    0.001
}
fn default_bg() -> [f32; 3] {
    [0.05, 0.05, 0.08]
}
fn default_ambient() -> f32 {
    0.15
}
fn default_light_dir() -> [f32; 3] {
    [0.577, 0.577, -0.577]
}
fn default_light_color() -> [f32; 3] {
    [1.0, 0.95, 0.9]
}
fn default_shadow_k() -> f32 {
    16.0
}
fn default_true() -> bool {
    true
}
fn default_width() -> u32 {
    512
}
fn default_height() -> u32 {
    512
}

impl Default for SceneSettings {
    fn default() -> Self {
        Self {
            camera_pos: default_camera_pos(),
            camera_target: [0.0; 3],
            fov: default_fov(),
            max_steps: default_max_steps(),
            max_dist: default_max_dist(),
            epsilon: default_epsilon(),
            background: default_bg(),
            ambient: default_ambient(),
            light_dir: default_light_dir(),
            light_color: default_light_color(),
            soft_shadows: false,
            shadow_k: default_shadow_k(),
            ao: true,
            width: default_width(),
            height: default_height(),
            time: 0.0,
        }
    }
}

// ─── SDF Node Tree ───────────────────────────────────────────────────

/// A node in the SDF tree. This is the IR that actors produce and
/// the code generator consumes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SdfNode {
    /// A primitive shape.
    Primitive {
        #[serde(flatten)]
        shape: SdfPrimitive,
        #[serde(default)]
        material: Option<SdfMaterial>,
    },

    /// A CSG operation combining two children.
    Operation {
        #[serde(flatten)]
        op: SdfOp,
        left: Box<SdfNode>,
        right: Box<SdfNode>,
    },

    /// A domain transform applied to a child.
    Transform {
        #[serde(flatten)]
        transform: SdfTransform,
        child: Box<SdfNode>,
    },

    /// Override material on a child.
    Material {
        material: SdfMaterial,
        child: Box<SdfNode>,
    },

    /// Named reference — resolved during graph composition.
    /// Actors output their SDF as a named node; downstream actors
    /// reference it by name.
    Ref { name: String },

    /// The complete scene: root SDF + render settings.
    Scene {
        root: Box<SdfNode>,
        settings: SceneSettings,
    },
}

impl SdfNode {
    // ── Primitive constructors ────────────────────────────────────

    pub fn sphere(radius: f32) -> Self {
        SdfNode::Primitive {
            shape: SdfPrimitive::Sphere { radius },
            material: None,
        }
    }

    pub fn cube(size: f32) -> Self {
        SdfNode::Primitive {
            shape: SdfPrimitive::Box { size: [size; 3] },
            material: None,
        }
    }

    pub fn box3(size: [f32; 3]) -> Self {
        SdfNode::Primitive {
            shape: SdfPrimitive::Box { size },
            material: None,
        }
    }

    pub fn cylinder(radius: f32, height: f32) -> Self {
        SdfNode::Primitive {
            shape: SdfPrimitive::Cylinder { radius, height },
            material: None,
        }
    }

    pub fn torus(major: f32, minor: f32) -> Self {
        SdfNode::Primitive {
            shape: SdfPrimitive::Torus {
                major_radius: major,
                minor_radius: minor,
            },
            material: None,
        }
    }

    pub fn capsule(radius: f32, height: f32) -> Self {
        SdfNode::Primitive {
            shape: SdfPrimitive::Capsule { radius, height },
            material: None,
        }
    }

    pub fn plane(normal: [f32; 3], offset: f32) -> Self {
        SdfNode::Primitive {
            shape: SdfPrimitive::Plane { normal, offset },
            material: None,
        }
    }

    pub fn cone(angle: f32, height: f32) -> Self {
        SdfNode::Primitive {
            shape: SdfPrimitive::Cone { angle, height },
            material: None,
        }
    }

    pub fn tube_path(points: Vec<[f32; 3]>, radii: Vec<f32>, k: f32) -> Self {
        SdfNode::Primitive {
            shape: SdfPrimitive::TubePath { points, radii, k },
            material: None,
        }
    }

    pub fn tapered_capsule(a: [f32; 3], b: [f32; 3], radius_a: f32, radius_b: f32) -> Self {
        SdfNode::Primitive {
            shape: SdfPrimitive::TaperedCapsule {
                a,
                b,
                radius_a,
                radius_b,
            },
            material: None,
        }
    }

    // ── Operation constructors ───────────────────────────────────

    pub fn union(a: SdfNode, b: SdfNode) -> Self {
        SdfNode::Operation {
            op: SdfOp::Union,
            left: Box::new(a),
            right: Box::new(b),
        }
    }

    pub fn intersection(a: SdfNode, b: SdfNode) -> Self {
        SdfNode::Operation {
            op: SdfOp::Intersection,
            left: Box::new(a),
            right: Box::new(b),
        }
    }

    pub fn difference(a: SdfNode, b: SdfNode) -> Self {
        SdfNode::Operation {
            op: SdfOp::Difference,
            left: Box::new(a),
            right: Box::new(b),
        }
    }

    pub fn smooth_union(a: SdfNode, b: SdfNode, k: f32) -> Self {
        SdfNode::Operation {
            op: SdfOp::SmoothUnion { k },
            left: Box::new(a),
            right: Box::new(b),
        }
    }

    pub fn smooth_intersection(a: SdfNode, b: SdfNode, k: f32) -> Self {
        SdfNode::Operation {
            op: SdfOp::SmoothIntersection { k },
            left: Box::new(a),
            right: Box::new(b),
        }
    }

    pub fn smooth_difference(a: SdfNode, b: SdfNode, k: f32) -> Self {
        SdfNode::Operation {
            op: SdfOp::SmoothDifference { k },
            left: Box::new(a),
            right: Box::new(b),
        }
    }

    // ── Transform constructors ───────────────────────────────────

    pub fn translate(self, offset: [f32; 3]) -> Self {
        SdfNode::Transform {
            transform: SdfTransform::Translate { offset },
            child: Box::new(self),
        }
    }

    pub fn rotate(self, angles: [f32; 3]) -> Self {
        SdfNode::Transform {
            transform: SdfTransform::Rotate { angles },
            child: Box::new(self),
        }
    }

    pub fn scale(self, factor: [f32; 3]) -> Self {
        SdfNode::Transform {
            transform: SdfTransform::Scale { factor },
            child: Box::new(self),
        }
    }

    pub fn scale_uniform(self, s: f32) -> Self {
        self.scale([s, s, s])
    }

    pub fn twist(self, strength: f32) -> Self {
        SdfNode::Transform {
            transform: SdfTransform::Twist { strength },
            child: Box::new(self),
        }
    }

    pub fn bend(self, strength: f32) -> Self {
        SdfNode::Transform {
            transform: SdfTransform::Bend { strength },
            child: Box::new(self),
        }
    }

    pub fn round(self, radius: f32) -> Self {
        SdfNode::Transform {
            transform: SdfTransform::Round { radius },
            child: Box::new(self),
        }
    }

    pub fn shell(self, thickness: f32) -> Self {
        SdfNode::Transform {
            transform: SdfTransform::Shell { thickness },
            child: Box::new(self),
        }
    }

    pub fn repeat(self, spacing: [f32; 3], count: [u32; 3]) -> Self {
        SdfNode::Transform {
            transform: SdfTransform::Repeat { spacing, count },
            child: Box::new(self),
        }
    }

    pub fn mirror(self, axis: [f32; 3]) -> Self {
        SdfNode::Transform {
            transform: SdfTransform::Mirror { axis },
            child: Box::new(self),
        }
    }

    pub fn displace(self, frequency: f32, amplitude: f32, octaves: u32) -> Self {
        SdfNode::Transform {
            transform: SdfTransform::Displace {
                frequency,
                amplitude,
                octaves,
            },
            child: Box::new(self),
        }
    }

    // ── Material ─────────────────────────────────────────────────

    pub fn with_material(self, material: SdfMaterial) -> Self {
        SdfNode::Material {
            material,
            child: Box::new(self),
        }
    }

    pub fn with_color(self, r: f32, g: f32, b: f32) -> Self {
        self.with_material(SdfMaterial {
            color: [r, g, b],
            ..Default::default()
        })
    }

    // ── Scene ────────────────────────────────────────────────────

    pub fn into_scene(self) -> Self {
        SdfNode::Scene {
            root: Box::new(self),
            settings: SceneSettings::default(),
        }
    }

    pub fn into_scene_with(self, settings: SceneSettings) -> Self {
        SdfNode::Scene {
            root: Box::new(self),
            settings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_api() {
        let scene = SdfNode::smooth_union(
            SdfNode::sphere(1.0)
                .translate([0.0, 0.5, 0.0])
                .with_color(1.0, 0.2, 0.1),
            SdfNode::cube(1.0).twist(0.5),
            0.3,
        )
        .into_scene();

        let json = serde_json::to_string_pretty(&scene).unwrap();
        assert!(json.contains("smoothUnion"));
        assert!(json.contains("sphere"));
        assert!(json.contains("\"twist\""));
    }

    #[test]
    fn test_roundtrip() {
        let node = SdfNode::smooth_union(
            SdfNode::sphere(1.0).translate([1.0, 0.0, 0.0]),
            SdfNode::torus(1.5, 0.3),
            0.4,
        );
        let json = serde_json::to_string(&node).unwrap();
        let _parsed: SdfNode = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_default_scene_settings() {
        let s = SceneSettings::default();
        assert_eq!(s.max_steps, 128);
        assert_eq!(s.width, 512);
        assert!(s.ao);
    }
}
