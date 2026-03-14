//! Biquad (second-order IIR) filter.
//!
//! A single `Biquad` struct covers low-pass, high-pass, band-pass, notch,
//! peaking EQ, low-shelf, and high-shelf filters. Actors configure the
//! filter type via [`BiquadCoeffs::design`] and then call [`Biquad::process`]
//! on each sample chunk.
//!
//! All math is `f64` internally for coefficient precision; samples are `f32`.

use std::f64::consts::PI;

/// Biquad filter types that can be designed from (frequency, Q, gain, sample_rate).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterType {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    /// Peaking EQ — boost/cut at center frequency.
    PeakingEQ,
    LowShelf,
    HighShelf,
    /// All-pass — phase shift only, useful for phaser effects.
    AllPass,
}

/// Normalized biquad coefficients in Direct Form I.
///
/// Transfer function: H(z) = (b0 + b1*z^-1 + b2*z^-2) / (1 + a1*z^-1 + a2*z^-2)
///
/// Coefficients are pre-divided by a0 so the denominator leading term is always 1.
#[derive(Debug, Clone, Copy)]
pub struct BiquadCoeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl BiquadCoeffs {
    /// Design biquad coefficients using the Audio EQ Cookbook formulas.
    ///
    /// - `filter_type`: which filter shape
    /// - `sample_rate`: in Hz
    /// - `freq`: center/cutoff frequency in Hz
    /// - `q`: quality factor (resonance). Typical range 0.1–20. Use 0.707 for Butterworth.
    /// - `gain_db`: only used for PeakingEQ, LowShelf, HighShelf (boost/cut in dB)
    pub fn design(
        filter_type: FilterType,
        sample_rate: f64,
        freq: f64,
        q: f64,
        gain_db: f64,
    ) -> Self {
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        // Shelf gain
        let a = 10.0_f64.powf(gain_db / 40.0); // sqrt of linear gain

        let (b0, b1, b2, a0, a1, a2) = match filter_type {
            FilterType::LowPass => {
                let b1 = 1.0 - cos_w0;
                let b0 = b1 / 2.0;
                let b2 = b0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::HighPass => {
                let b1 = -(1.0 + cos_w0);
                let b0 = -b1 / 2.0;
                let b2 = b0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::BandPass => {
                let b0 = alpha;
                let b1 = 0.0;
                let b2 = -alpha;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::Notch => {
                let b0 = 1.0;
                let b1 = -2.0 * cos_w0;
                let b2 = 1.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::PeakingEQ => {
                let b0 = 1.0 + alpha * a;
                let b1 = -2.0 * cos_w0;
                let b2 = 1.0 - alpha * a;
                let a0 = 1.0 + alpha / a;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha / a;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::LowShelf => {
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
                let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
                let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
                let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
                let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
                let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::HighShelf => {
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
                let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
                let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
                let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
                let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
                let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::AllPass => {
                let b0 = 1.0 - alpha;
                let b1 = -2.0 * cos_w0;
                let b2 = 1.0 + alpha;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
        };

        // Normalize by a0
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }
}

/// Stateful biquad filter that processes f32 samples.
///
/// Maintains two samples of delay state (Direct Form II Transposed).
/// Create one per channel for multi-channel audio.
#[derive(Debug, Clone)]
pub struct Biquad {
    coeffs: BiquadCoeffs,
    /// Delay line state.
    z1: f64,
    z2: f64,
}

impl Biquad {
    pub fn new(coeffs: BiquadCoeffs) -> Self {
        Self {
            coeffs,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Update coefficients (e.g. for parameter automation) without resetting state.
    pub fn set_coeffs(&mut self, coeffs: BiquadCoeffs) {
        self.coeffs = coeffs;
    }

    /// Reset delay line to zero (call on stream Begin or discontinuity).
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// Process a single sample. Direct Form II Transposed.
    #[inline]
    pub fn process_sample(&mut self, input: f32) -> f32 {
        let x = input as f64;
        let c = &self.coeffs;
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        y as f32
    }

    /// Process a slice of samples in-place.
    #[inline]
    pub fn process(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            *s = self.process_sample(*s);
        }
    }

    /// Process interleaved multi-channel audio on a specific channel.
    ///
    /// `channels`: total channel count (e.g. 2 for stereo)
    /// `channel`: which channel this filter handles (0-indexed)
    pub fn process_interleaved(&mut self, samples: &mut [f32], channels: usize, channel: usize) {
        for s in samples.iter_mut().skip(channel).step_by(channels) {
            *s = self.process_sample(*s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lowpass_dc_passthrough() {
        // A low-pass filter should pass DC (0 Hz) at unity gain.
        let coeffs = BiquadCoeffs::design(FilterType::LowPass, 44100.0, 1000.0, 0.707, 0.0);
        let mut filter = Biquad::new(coeffs);

        // Feed 1000 samples of DC=1.0 to let the filter settle
        let mut samples = vec![1.0f32; 1000];
        filter.process(&mut samples);

        // Last sample should be very close to 1.0
        assert!((samples[999] - 1.0).abs() < 0.001, "DC should pass through LPF");
    }

    #[test]
    fn test_lowpass_attenuates_high_freq() {
        let coeffs = BiquadCoeffs::design(FilterType::LowPass, 44100.0, 100.0, 0.707, 0.0);
        let mut filter = Biquad::new(coeffs);

        // Generate a 10kHz sine (well above 100Hz cutoff)
        let freq = 10000.0;
        let sr = 44100.0;
        let mut samples: Vec<f32> = (0..4410)
            .map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / sr).sin() as f32)
            .collect();

        filter.process(&mut samples);

        // RMS of filtered signal should be much less than input RMS (~0.707)
        let rms: f32 = (samples[2000..].iter().map(|s| s * s).sum::<f32>()
            / (samples.len() - 2000) as f32)
            .sqrt();
        assert!(rms < 0.05, "10kHz should be heavily attenuated by 100Hz LPF, got rms={}", rms);
    }

    #[test]
    fn test_highpass_blocks_dc() {
        let coeffs = BiquadCoeffs::design(FilterType::HighPass, 44100.0, 1000.0, 0.707, 0.0);
        let mut filter = Biquad::new(coeffs);

        let mut samples = vec![1.0f32; 2000];
        filter.process(&mut samples);

        assert!(
            samples[1999].abs() < 0.001,
            "DC should be blocked by HPF, got {}",
            samples[1999]
        );
    }

    #[test]
    fn test_notch_rejects_center() {
        let center = 1000.0;
        let sr = 44100.0;
        let coeffs = BiquadCoeffs::design(FilterType::Notch, sr, center, 10.0, 0.0);
        let mut filter = Biquad::new(coeffs);

        // Generate 1kHz sine
        let mut samples: Vec<f32> = (0..4410)
            .map(|i| (2.0 * PI * center * i as f64 / sr).sin() as f32)
            .collect();

        filter.process(&mut samples);

        let rms: f32 = (samples[2000..].iter().map(|s| s * s).sum::<f32>()
            / (samples.len() - 2000) as f32)
            .sqrt();
        assert!(rms < 0.05, "Center frequency should be rejected by notch, got rms={}", rms);
    }

    #[test]
    fn test_peaking_eq_boost() {
        let sr = 44100.0;
        let freq = 1000.0;
        let gain_db = 12.0;
        let coeffs = BiquadCoeffs::design(FilterType::PeakingEQ, sr, freq, 1.0, gain_db);
        let mut filter = Biquad::new(coeffs);

        let mut samples: Vec<f32> = (0..4410)
            .map(|i| (2.0 * PI * freq * i as f64 / sr).sin() as f32)
            .collect();

        filter.process(&mut samples);

        // RMS should be boosted above input RMS of ~0.707
        let rms: f32 = (samples[2000..].iter().map(|s| s * s).sum::<f32>()
            / (samples.len() - 2000) as f32)
            .sqrt();
        // 12dB boost ≈ 4x linear gain → rms should be around 2.8
        assert!(rms > 2.0, "12dB peaking EQ should significantly boost, got rms={}", rms);
    }

    #[test]
    fn test_reset_clears_state() {
        let coeffs = BiquadCoeffs::design(FilterType::LowPass, 44100.0, 1000.0, 0.707, 0.0);
        let mut filter = Biquad::new(coeffs);

        filter.process(&mut [1.0; 100]);
        filter.reset();

        // After reset, processing 0.0 should yield 0.0 (no ringing from prior state)
        assert_eq!(filter.process_sample(0.0), 0.0);
    }

    #[test]
    fn test_interleaved_stereo() {
        let coeffs = BiquadCoeffs::design(FilterType::LowPass, 44100.0, 100.0, 0.707, 0.0);
        let mut left = Biquad::new(coeffs);
        let mut right = Biquad::new(coeffs);

        // Interleaved stereo: [L, R, L, R, ...]
        // Left = DC 1.0, Right = 10kHz sine
        let sr = 44100.0;
        let n = 2000;
        let mut samples: Vec<f32> = (0..n)
            .flat_map(|i| {
                let l = 1.0f32;
                let r = (2.0 * PI * 10000.0 * i as f64 / sr).sin() as f32;
                [l, r]
            })
            .collect();

        left.process_interleaved(&mut samples, 2, 0);
        right.process_interleaved(&mut samples, 2, 1);

        // Left (DC) should be ~1.0 at end
        let last_left = samples[(n - 1) * 2];
        assert!((last_left - 1.0).abs() < 0.01, "Left DC should pass, got {}", last_left);

        // Right (10kHz through 100Hz LPF) should be near zero
        let right_rms: f32 = (samples[2000..]
            .iter()
            .skip(1)
            .step_by(2)
            .map(|s| s * s)
            .sum::<f32>()
            / (n - 1000) as f32)
            .sqrt();
        assert!(right_rms < 0.05, "Right 10kHz should be attenuated, got rms={}", right_rms);
    }
}
