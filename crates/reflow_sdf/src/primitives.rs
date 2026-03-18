//! CPU-side SDF evaluation for testing, bounding boxes, and picking.
//!
//! These match the WGSL functions exactly so tests can verify correctness
//! without GPU access.

/// Signed distance to a sphere centered at origin.
pub fn sphere(p: [f32; 3], radius: f32) -> f32 {
    length(p) - radius
}

/// Signed distance to an axis-aligned box centered at origin.
pub fn box_sdf(p: [f32; 3], size: [f32; 3]) -> f32 {
    let q = [
        p[0].abs() - size[0],
        p[1].abs() - size[1],
        p[2].abs() - size[2],
    ];
    length([q[0].max(0.0), q[1].max(0.0), q[2].max(0.0)]) + q[0].max(q[1].max(q[2])).min(0.0)
}

/// Signed distance to a torus on the XZ plane.
pub fn torus(p: [f32; 3], major: f32, minor: f32) -> f32 {
    let q = [(p[0] * p[0] + p[2] * p[2]).sqrt() - major, p[1]];
    length2(q) - minor
}

/// Signed distance to a cylinder along Y axis.
pub fn cylinder(p: [f32; 3], radius: f32, height: f32) -> f32 {
    let d = [
        (p[0] * p[0] + p[2] * p[2]).sqrt() - radius,
        p[1].abs() - height,
    ];
    d[0].max(d[1]).min(0.0) + length2([d[0].max(0.0), d[1].max(0.0)])
}

/// Signed distance to a plane with given normal and offset.
pub fn plane(p: [f32; 3], normal: [f32; 3], offset: f32) -> f32 {
    p[0] * normal[0] + p[1] * normal[1] + p[2] * normal[2] + offset
}

/// Smooth minimum (polynomial).
pub fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b * (1.0 - h) + a * h - k * h * (1.0 - h)
}

/// Smooth maximum.
pub fn smax(a: f32, b: f32, k: f32) -> f32 {
    -smin(-a, -b, k)
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn length2(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sphere_origin() {
        assert!((sphere([0.0, 0.0, 0.0], 1.0) - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_sphere_surface() {
        assert!(sphere([1.0, 0.0, 0.0], 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_sphere_outside() {
        assert!(sphere([2.0, 0.0, 0.0], 1.0) > 0.0);
    }

    #[test]
    fn test_box_origin() {
        assert!(box_sdf([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]) < 0.0);
    }

    #[test]
    fn test_box_surface() {
        assert!(box_sdf([1.0, 0.0, 0.0], [1.0, 1.0, 1.0]).abs() < 1e-6);
    }

    #[test]
    fn test_torus() {
        // Point on torus surface (major=2, minor=0.5): at (2, 0, 0)
        assert!(torus([2.5, 0.0, 0.0], 2.0, 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_plane() {
        assert!((plane([0.0, 1.0, 0.0], [0.0, 1.0, 0.0], 0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_smin() {
        // When a << b, smin ≈ a
        assert!((smin(0.1, 5.0, 0.5) - 0.1).abs() < 0.15);
        // smin always ≤ min(a, b)
        assert!(smin(1.0, 2.0, 0.5) <= 1.0);
    }

    #[test]
    fn test_cylinder() {
        // Inside
        assert!(cylinder([0.0, 0.0, 0.0], 1.0, 1.0) < 0.0);
        // On surface (radius)
        assert!(cylinder([1.0, 0.0, 0.0], 1.0, 1.0).abs() < 1e-5);
    }
}
