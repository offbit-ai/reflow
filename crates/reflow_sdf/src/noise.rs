//! CPU-side noise functions for procedural generation.
//!
//! These match the WGSL noise functions in codegen and provide CPU-side
//! evaluation for heightmaps, terrain, texture generation, etc.

/// 3D hash function (matches WGSL hash3).
fn hash3(x: f64, y: f64, z: f64) -> f64 {
    let mut qx = (x * 0.1031).fract();
    let mut qy = (y * 0.1031).fract();
    let mut qz = (z * 0.1031).fract();
    let dot = qx * (qy + 19.19) + qy * (qz + 19.19) + qz * (qx + 19.19);
    qx += dot;
    qy += dot;
    qz += dot;
    ((qx + qy) * qz).fract()
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
fn smoothstep(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise (3D). Returns 0.0–1.0.
pub fn value_noise(x: f64, y: f64, z: f64) -> f64 {
    let ix = x.floor();
    let iy = y.floor();
    let iz = z.floor();
    let fx = smoothstep(x - ix);
    let fy = smoothstep(y - iy);
    let fz = smoothstep(z - iz);

    lerp(
        lerp(
            lerp(hash3(ix, iy, iz), hash3(ix + 1.0, iy, iz), fx),
            lerp(hash3(ix, iy + 1.0, iz), hash3(ix + 1.0, iy + 1.0, iz), fx),
            fy,
        ),
        lerp(
            lerp(hash3(ix, iy, iz + 1.0), hash3(ix + 1.0, iy, iz + 1.0), fx),
            lerp(
                hash3(ix, iy + 1.0, iz + 1.0),
                hash3(ix + 1.0, iy + 1.0, iz + 1.0),
                fx,
            ),
            fy,
        ),
        fz,
    )
}

/// 2D value noise. Returns 0.0–1.0.
pub fn value_noise_2d(x: f64, y: f64) -> f64 {
    value_noise(x, y, 0.0)
}

/// Perlin gradient noise (2D). Returns approximately -1.0–1.0.
pub fn perlin_2d(x: f64, y: f64) -> f64 {
    let ix = x.floor() as i64;
    let iy = y.floor() as i64;
    let fx = x - ix as f64;
    let fy = y - iy as f64;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);

    let grad = |ix: i64, iy: i64, dx: f64, dy: f64| -> f64 {
        let h = ((ix * 374761393 + iy * 668265263) as u64).wrapping_mul(1274126177) >> 30;
        match h & 3 {
            0 => dx + dy,
            1 => -dx + dy,
            2 => dx - dy,
            _ => -dx - dy,
        }
    };

    let n00 = grad(ix, iy, fx, fy);
    let n10 = grad(ix + 1, iy, fx - 1.0, fy);
    let n01 = grad(ix, iy + 1, fx, fy - 1.0);
    let n11 = grad(ix + 1, iy + 1, fx - 1.0, fy - 1.0);

    lerp(lerp(n00, n10, ux), lerp(n01, n11, ux), uy)
}

/// Simplex noise (2D). Returns approximately -1.0–1.0.
pub fn simplex_2d(x: f64, y: f64) -> f64 {
    const F2: f64 = 0.3660254037844386; // (sqrt(3) - 1) / 2
    const G2: f64 = 0.21132486540518713; // (3 - sqrt(3)) / 6

    let s = (x + y) * F2;
    let i = (x + s).floor();
    let j = (y + s).floor();
    let t = (i + j) * G2;
    let x0 = x - (i - t);
    let y0 = y - (j - t);

    let (i1, j1) = if x0 > y0 { (1.0, 0.0) } else { (0.0, 1.0) };
    let x1 = x0 - i1 + G2;
    let y1 = y0 - j1 + G2;
    let x2 = x0 - 1.0 + 2.0 * G2;
    let y2 = y0 - 1.0 + 2.0 * G2;

    let gi = |ix: f64, iy: f64| -> usize {
        let h = hash3(ix, iy, 0.0);
        (h * 8.0) as usize % 8
    };

    const GRAD: [[f64; 2]; 8] = [
        [1.0, 0.0],
        [-1.0, 0.0],
        [0.0, 1.0],
        [0.0, -1.0],
        [0.707, 0.707],
        [-0.707, 0.707],
        [0.707, -0.707],
        [-0.707, -0.707],
    ];

    let contrib = |t_val: f64, gx: f64, gy: f64, dx: f64, dy: f64| -> f64 {
        let t = t_val - dx * dx - dy * dy;
        if t < 0.0 {
            0.0
        } else {
            t * t * t * t * (gx * dx + gy * dy)
        }
    };

    let g0 = &GRAD[gi(i, j)];
    let g1 = &GRAD[gi(i + i1, j + j1)];
    let g2 = &GRAD[gi(i + 1.0, j + 1.0)];

    let n0 = contrib(0.5, g0[0], g0[1], x0, y0);
    let n1 = contrib(0.5, g1[0], g1[1], x1, y1);
    let n2 = contrib(0.5, g2[0], g2[1], x2, y2);

    70.0 * (n0 + n1 + n2)
}

/// Worley (cellular) noise (2D). Returns distance to nearest cell point (0.0–~1.0).
pub fn worley_2d(x: f64, y: f64) -> f64 {
    let ix = x.floor();
    let iy = y.floor();
    let fx = x - ix;
    let fy = y - iy;

    let mut min_dist = f64::MAX;

    for dy in -1..=1 {
        for dx in -1..=1 {
            let nx = ix + dx as f64;
            let ny = iy + dy as f64;
            // Cell point position (jittered)
            let px = dx as f64 + hash3(nx, ny, 0.0);
            let py = dy as f64 + hash3(nx, ny, 1.0);
            let d = (fx - px).powi(2) + (fy - py).powi(2);
            if d < min_dist {
                min_dist = d;
            }
        }
    }

    min_dist.sqrt()
}

/// Fractal Brownian Motion (FBM). Layers octaves of a base noise function.
pub fn fbm_2d<F: Fn(f64, f64) -> f64>(
    x: f64,
    y: f64,
    octaves: u32,
    lacunarity: f64,
    persistence: f64,
    noise_fn: F,
) -> f64 {
    let mut value = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    let mut max_amp = 0.0;

    for _ in 0..octaves {
        value += amplitude * noise_fn(x * frequency, y * frequency);
        max_amp += amplitude;
        amplitude *= persistence;
        frequency *= lacunarity;
    }

    value / max_amp
}

/// Ridged multi-fractal noise. Like FBM but with abs() for ridge lines.
pub fn ridged_2d<F: Fn(f64, f64) -> f64>(
    x: f64,
    y: f64,
    octaves: u32,
    lacunarity: f64,
    persistence: f64,
    noise_fn: F,
) -> f64 {
    let mut value = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    let mut weight = 1.0;

    for _ in 0..octaves {
        let signal = 1.0 - noise_fn(x * frequency, y * frequency).abs();
        let signal = signal * signal * weight;
        weight = (signal * 2.0).clamp(0.0, 1.0);
        value += signal * amplitude;
        amplitude *= persistence;
        frequency *= lacunarity;
    }

    value
}

/// Generate a 2D noise grid. Returns `width * height` floats.
pub fn generate_noise_grid(
    width: usize,
    height: usize,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    noise_fn: impl Fn(f64, f64) -> f64,
) -> Vec<f64> {
    let mut grid = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let nx = offset_x + (x as f64 / width as f64) * scale;
            let ny = offset_y + (y as f64 / height as f64) * scale;
            grid.push(noise_fn(nx, ny));
        }
    }
    grid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_noise_range() {
        for i in 0..100 {
            let v = value_noise_2d(i as f64 * 0.1, i as f64 * 0.13);
            assert!(v >= 0.0 && v <= 1.0, "value_noise out of range: {}", v);
        }
    }

    #[test]
    fn test_perlin_continuity() {
        let a = perlin_2d(1.0, 1.0);
        let b = perlin_2d(1.001, 1.0);
        assert!(
            (a - b).abs() < 0.1,
            "Perlin should be continuous: {} vs {}",
            a,
            b
        );
    }

    #[test]
    fn test_worley_positive() {
        for i in 0..50 {
            let v = worley_2d(i as f64 * 0.3, i as f64 * 0.7);
            assert!(v >= 0.0, "Worley should be non-negative: {}", v);
        }
    }

    #[test]
    fn test_fbm_bounded() {
        let v = fbm_2d(5.0, 3.0, 6, 2.0, 0.5, perlin_2d);
        assert!(v.abs() < 2.0, "FBM should be bounded: {}", v);
    }

    #[test]
    fn test_generate_grid() {
        let grid = generate_noise_grid(64, 64, 4.0, 0.0, 0.0, value_noise_2d);
        assert_eq!(grid.len(), 64 * 64);
    }
}
