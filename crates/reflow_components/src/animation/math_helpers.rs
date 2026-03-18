//! Shared math utilities for animation actors.
//!
//! All matrices are column-major f32 arrays (matching GPU convention).
//! Quaternions are [x, y, z, w].

/// 4×4 identity matrix.
pub const MAT4_IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// Multiply two column-major 4×4 matrices: result = a * b.
pub fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut r = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[k * 4 + row] * b[col * 4 + k];
            }
            r[col * 4 + row] = sum;
        }
    }
    r
}

/// Compose a TRS (translate, rotate, scale) into a column-major mat4.
/// Rotation is quaternion [x, y, z, w].
pub fn trs_to_mat4(pos: [f32; 3], rot: [f32; 4], scl: [f32; 3]) -> [f32; 16] {
    let r = quat_to_mat3(rot);
    [
        r[0] * scl[0],
        r[1] * scl[0],
        r[2] * scl[0],
        0.0,
        r[3] * scl[1],
        r[4] * scl[1],
        r[5] * scl[1],
        0.0,
        r[6] * scl[2],
        r[7] * scl[2],
        r[8] * scl[2],
        0.0,
        pos[0],
        pos[1],
        pos[2],
        1.0,
    ]
}

/// Quaternion [x,y,z,w] → 3×3 rotation matrix (column-major, 9 floats).
pub fn quat_to_mat3(q: [f32; 4]) -> [f32; 9] {
    let [x, y, z, w] = q;
    let x2 = x + x;
    let y2 = y + y;
    let z2 = z + z;
    let xx = x * x2;
    let xy = x * y2;
    let xz = x * z2;
    let yy = y * y2;
    let yz = y * z2;
    let zz = z * z2;
    let wx = w * x2;
    let wy = w * y2;
    let wz = w * z2;
    [
        1.0 - yy - zz,
        xy + wz,
        xz - wy,
        xy - wz,
        1.0 - xx - zz,
        yz + wx,
        xz + wy,
        yz - wx,
        1.0 - xx - yy,
    ]
}

/// Invert a column-major 4×4 matrix. Returns identity if singular.
pub fn mat4_inverse(m: &[f32; 16]) -> [f32; 16] {
    let mut inv = [0.0f32; 16];

    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
        + m[9] * m[7] * m[14]
        + m[13] * m[6] * m[11]
        - m[13] * m[7] * m[10];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
        - m[8] * m[7] * m[14]
        - m[12] * m[6] * m[11]
        + m[12] * m[7] * m[10];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
        + m[8] * m[7] * m[13]
        + m[12] * m[5] * m[11]
        - m[12] * m[7] * m[9];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
        - m[8] * m[6] * m[13]
        - m[12] * m[5] * m[10]
        + m[12] * m[6] * m[9];

    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-10 {
        return MAT4_IDENTITY;
    }

    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
        - m[9] * m[3] * m[14]
        - m[13] * m[2] * m[11]
        + m[13] * m[3] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
        + m[8] * m[3] * m[14]
        + m[12] * m[2] * m[11]
        - m[12] * m[3] * m[10];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
        - m[8] * m[3] * m[13]
        - m[12] * m[1] * m[11]
        + m[12] * m[3] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
        + m[8] * m[2] * m[13]
        + m[12] * m[1] * m[10]
        - m[12] * m[2] * m[9];

    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
        + m[5] * m[3] * m[14]
        + m[13] * m[2] * m[7]
        - m[13] * m[3] * m[6];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
        - m[4] * m[3] * m[14]
        - m[12] * m[2] * m[7]
        + m[12] * m[3] * m[6];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
        + m[4] * m[3] * m[13]
        + m[12] * m[1] * m[7]
        - m[12] * m[3] * m[5];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
        - m[4] * m[2] * m[13]
        - m[12] * m[1] * m[6]
        + m[12] * m[2] * m[5];

    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
        - m[5] * m[3] * m[10]
        - m[9] * m[2] * m[7]
        + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
        + m[4] * m[3] * m[10]
        + m[8] * m[2] * m[7]
        - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
        - m[4] * m[3] * m[9]
        - m[8] * m[1] * m[7]
        + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
        + m[4] * m[2] * m[9]
        + m[8] * m[1] * m[6]
        - m[8] * m[2] * m[5];

    let inv_det = 1.0 / det;
    for i in 0..16 {
        inv[i] *= inv_det;
    }
    inv
}

/// Transform a point (w=1) by a column-major mat4.
pub fn mat4_transform_point(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// Transform a direction (w=0) by a column-major mat4 upper-left 3×3.
pub fn mat4_transform_dir(m: &[f32; 16], d: [f32; 3]) -> [f32; 3] {
    [
        m[0] * d[0] + m[4] * d[1] + m[8] * d[2],
        m[1] * d[0] + m[5] * d[1] + m[9] * d[2],
        m[2] * d[0] + m[6] * d[1] + m[10] * d[2],
    ]
}

/// Normalize a 3D vector. Returns zero vector if length < epsilon.
pub fn vec3_normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 1e-8 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0; 3]
    }
}

/// Lerp between two vec3 values.
pub fn vec3_lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Slerp between two unit quaternions [x,y,z,w].
pub fn quat_slerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let mut b = b;
    if dot < 0.0 {
        b = [-b[0], -b[1], -b[2], -b[3]];
        dot = -dot;
    }
    if dot > 0.9995 {
        let r = [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ];
        let l = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2] + r[3] * r[3]).sqrt();
        return [r[0] / l, r[1] / l, r[2] / l, r[3] / l];
    }
    let theta = dot.clamp(-1.0, 1.0).acos();
    let sin_theta = theta.sin();
    let sa = ((1.0 - t) * theta).sin() / sin_theta;
    let sb = (t * theta).sin() / sin_theta;
    [
        sa * a[0] + sb * b[0],
        sa * a[1] + sb * b[1],
        sa * a[2] + sb * b[2],
        sa * a[3] + sb * b[3],
    ]
}

/// Normalize a quaternion.
pub fn quat_normalize(q: [f32; 4]) -> [f32; 4] {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if l > 1e-8 {
        [q[0] / l, q[1] / l, q[2] / l, q[3] / l]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mat4_identity_inverse() {
        let inv = mat4_inverse(&MAT4_IDENTITY);
        for i in 0..16 {
            assert!((inv[i] - MAT4_IDENTITY[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn test_trs_identity() {
        let m = trs_to_mat4([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
        for i in 0..16 {
            assert!(
                (m[i] - MAT4_IDENTITY[i]).abs() < 1e-6,
                "i={} got {}",
                i,
                m[i]
            );
        }
    }

    #[test]
    fn test_trs_translate() {
        let m = trs_to_mat4([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
        let p = mat4_transform_point(&m, [0.0, 0.0, 0.0]);
        assert!((p[0] - 1.0).abs() < 1e-6);
        assert!((p[1] - 2.0).abs() < 1e-6);
        assert!((p[2] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_trs_scale() {
        let m = trs_to_mat4([0.0; 3], [0.0, 0.0, 0.0, 1.0], [2.0, 3.0, 4.0]);
        let p = mat4_transform_point(&m, [1.0, 1.0, 1.0]);
        assert!((p[0] - 2.0).abs() < 1e-6);
        assert!((p[1] - 3.0).abs() < 1e-6);
        assert!((p[2] - 4.0).abs() < 1e-6);
    }

    #[test]
    fn test_mat4_mul_inverse() {
        let m = trs_to_mat4([3.0, -1.0, 5.0], [0.0, 0.383, 0.0, 0.924], [1.0; 3]);
        let inv = mat4_inverse(&m);
        let id = mat4_mul(&m, &inv);
        for i in 0..16 {
            let expected = MAT4_IDENTITY[i];
            assert!(
                (id[i] - expected).abs() < 1e-4,
                "i={} expected {} got {}",
                i,
                expected,
                id[i]
            );
        }
    }

    #[test]
    fn test_quat_slerp_endpoints() {
        let a = [0.0, 0.0, 0.0, 1.0];
        let b = [0.0, 0.707, 0.0, 0.707];
        let r0 = quat_slerp(a, b, 0.0);
        let r1 = quat_slerp(a, b, 1.0);
        for i in 0..4 {
            assert!((r0[i] - a[i]).abs() < 1e-4);
            assert!((r1[i] - b[i]).abs() < 1e-4);
        }
    }
}
