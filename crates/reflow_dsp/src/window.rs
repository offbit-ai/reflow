//! Window functions for spectral analysis.
//!
//! Used by FFT, IFFT, PitchShift, TimeStretch, NoiseReduction, and
//! AudioSpectrum actors. All functions produce a window of length `n`
//! as a `Vec<f32>`.

use std::f64::consts::PI;

/// Window function type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowType {
    /// Rectangular (no windowing). Unity gain, worst spectral leakage.
    Rectangular,
    /// Hann window. Good general-purpose choice for spectral analysis.
    Hann,
    /// Hamming window. Slightly better sidelobe rejection than Hann.
    Hamming,
    /// Blackman window. Better sidelobe rejection, wider main lobe.
    Blackman,
    /// Blackman-Harris 4-term. Excellent sidelobe rejection.
    BlackmanHarris,
    /// Flat-top window. Best amplitude accuracy for frequency measurement.
    FlatTop,
}

/// Generate a window of length `n`.
pub fn generate(window_type: WindowType, n: usize) -> Vec<f32> {
    match window_type {
        WindowType::Rectangular => vec![1.0; n],
        WindowType::Hann => hann(n),
        WindowType::Hamming => hamming(n),
        WindowType::Blackman => blackman(n),
        WindowType::BlackmanHarris => blackman_harris(n),
        WindowType::FlatTop => flat_top(n),
    }
}

/// Apply a window to samples in-place.
///
/// `window` and `samples` must have the same length.
#[inline]
pub fn apply(samples: &mut [f32], window: &[f32]) {
    debug_assert_eq!(samples.len(), window.len());
    for (s, w) in samples.iter_mut().zip(window.iter()) {
        *s *= w;
    }
}

/// Compute the coherent gain (sum of window / n) for normalization.
pub fn coherent_gain(window: &[f32]) -> f32 {
    if window.is_empty() {
        return 0.0;
    }
    window.iter().sum::<f32>() / window.len() as f32
}

fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = 2.0 * PI * i as f64 / (n - 1).max(1) as f64;
            (0.5 * (1.0 - x.cos())) as f32
        })
        .collect()
}

fn hamming(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = 2.0 * PI * i as f64 / (n - 1).max(1) as f64;
            (0.54 - 0.46 * x.cos()) as f32
        })
        .collect()
}

fn blackman(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = 2.0 * PI * i as f64 / (n - 1).max(1) as f64;
            (0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos()) as f32
        })
        .collect()
}

fn blackman_harris(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = 2.0 * PI * i as f64 / (n - 1).max(1) as f64;
            (0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos()
                - 0.01168 * (3.0 * x).cos()) as f32
        })
        .collect()
}

fn flat_top(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = 2.0 * PI * i as f64 / (n - 1).max(1) as f64;
            (0.21557895 - 0.41663158 * x.cos() + 0.277263158 * (2.0 * x).cos()
                - 0.083578947 * (3.0 * x).cos()
                + 0.006947368 * (4.0 * x).cos()) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hann_endpoints_zero() {
        let w = generate(WindowType::Hann, 256);
        assert!(w[0].abs() < 1e-6, "Hann start should be ~0, got {}", w[0]);
        assert!(
            w[255].abs() < 1e-6,
            "Hann end should be ~0, got {}",
            w[255]
        );
    }

    #[test]
    fn test_hann_peak_at_center() {
        let w = generate(WindowType::Hann, 256);
        let mid = w[127];
        assert!((mid - 1.0).abs() < 0.01, "Hann center should be ~1.0, got {}", mid);
    }

    #[test]
    fn test_hamming_nonzero_endpoints() {
        let w = generate(WindowType::Hamming, 256);
        // Hamming has nonzero endpoints (~0.08)
        assert!(w[0] > 0.07 && w[0] < 0.09, "Hamming start should be ~0.08, got {}", w[0]);
    }

    #[test]
    fn test_rectangular_all_ones() {
        let w = generate(WindowType::Rectangular, 100);
        assert!(w.iter().all(|&v| (v - 1.0).abs() < 1e-6));
    }

    #[test]
    fn test_apply_window() {
        let w = generate(WindowType::Hann, 4);
        let mut samples = vec![1.0f32; 4];
        apply(&mut samples, &w);
        // First and last should be ~0
        assert!(samples[0].abs() < 1e-6);
        assert!(samples[3].abs() < 1e-6);
    }

    #[test]
    fn test_symmetry() {
        for wt in [
            WindowType::Hann,
            WindowType::Hamming,
            WindowType::Blackman,
            WindowType::BlackmanHarris,
        ] {
            let w = generate(wt, 256);
            for i in 0..128 {
                assert!(
                    (w[i] - w[255 - i]).abs() < 1e-5,
                    "{:?} not symmetric at {}: {} vs {}",
                    wt,
                    i,
                    w[i],
                    w[255 - i]
                );
            }
        }
    }

    #[test]
    fn test_coherent_gain_rectangular() {
        let w = generate(WindowType::Rectangular, 100);
        let gain = coherent_gain(&w);
        assert!((gain - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_coherent_gain_hann() {
        let w = generate(WindowType::Hann, 256);
        let gain = coherent_gain(&w);
        // Hann coherent gain ≈ 0.5
        assert!((gain - 0.5).abs() < 0.01, "Hann coherent gain should be ~0.5, got {}", gain);
    }
}
