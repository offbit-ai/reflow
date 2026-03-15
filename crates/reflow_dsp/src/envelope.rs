//! Envelope detector / follower.
//!
//! Tracks the amplitude envelope of a signal using configurable attack
//! and release times. Used by Compressor, Limiter, Gate, EnvelopeFollower,
//! and SilenceDetect actors.
//!
//! Supports both peak and RMS detection modes.

/// Detection mode for the envelope follower.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DetectionMode {
    /// Track instantaneous peak level.
    Peak,
    /// Track RMS level (smoothed power).
    Rms,
}

/// Envelope detector with configurable attack/release.
///
/// Output is a linear amplitude envelope (not dB). Use [`crate::db::linear_to_db`]
/// to convert if needed.
#[derive(Debug, Clone)]
pub struct EnvelopeDetector {
    attack_coeff: f64,
    release_coeff: f64,
    mode: DetectionMode,
    /// Current envelope value (linear).
    envelope: f64,
}

impl EnvelopeDetector {
    /// Create a new envelope detector.
    ///
    /// - `attack_ms`: attack time in milliseconds (how fast envelope rises)
    /// - `release_ms`: release time in milliseconds (how fast envelope falls)
    /// - `sample_rate`: in Hz
    /// - `mode`: Peak or RMS detection
    pub fn new(attack_ms: f64, release_ms: f64, sample_rate: f64, mode: DetectionMode) -> Self {
        Self {
            attack_coeff: time_constant(attack_ms, sample_rate),
            release_coeff: time_constant(release_ms, sample_rate),
            mode,
            envelope: 0.0,
        }
    }

    /// Update attack/release times without resetting state.
    pub fn set_times(&mut self, attack_ms: f64, release_ms: f64, sample_rate: f64) {
        self.attack_coeff = time_constant(attack_ms, sample_rate);
        self.release_coeff = time_constant(release_ms, sample_rate);
    }

    /// Reset envelope to zero.
    pub fn reset(&mut self) {
        self.envelope = 0.0;
    }

    /// Current envelope level (linear amplitude).
    pub fn level(&self) -> f64 {
        self.envelope
    }

    /// Process a single sample, return the current envelope level.
    #[inline]
    pub fn process_sample(&mut self, input: f32) -> f32 {
        let input_level = match self.mode {
            DetectionMode::Peak => (input as f64).abs(),
            DetectionMode::Rms => (input as f64) * (input as f64),
        };

        let coeff = if input_level > self.envelope {
            self.attack_coeff
        } else {
            self.release_coeff
        };

        self.envelope = coeff * self.envelope + (1.0 - coeff) * input_level;

        let output = match self.mode {
            DetectionMode::Peak => self.envelope,
            DetectionMode::Rms => self.envelope.sqrt(),
        };
        output as f32
    }

    /// Process a slice, writing envelope values into `output`.
    ///
    /// `output` must be the same length as `input`.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        debug_assert_eq!(input.len(), output.len());
        for (i, o) in input.iter().zip(output.iter_mut()) {
            *o = self.process_sample(*i);
        }
    }

    /// Process a slice, returning envelope values as a new Vec.
    pub fn process_to_vec(&mut self, input: &[f32]) -> Vec<f32> {
        let mut output = vec![0.0f32; input.len()];
        self.process(input, &mut output);
        output
    }
}

/// Compute the one-pole smoothing coefficient for a given time constant.
///
/// `time_ms` = 0 returns 0.0 (instant response).
/// Uses the standard analog RC time constant approximation:
/// coeff = exp(-1 / (time_in_samples))
fn time_constant(time_ms: f64, sample_rate: f64) -> f64 {
    if time_ms <= 0.0 {
        return 0.0;
    }
    let samples = time_ms * 0.001 * sample_rate;
    (-1.0 / samples).exp()
}

/// Dynamics processor built on top of [`EnvelopeDetector`].
///
/// Implements the gain computation for compressor, limiter, gate, and expander.
/// The actor wraps this and applies the computed gain to the audio samples.
#[derive(Debug, Clone)]
pub struct DynamicsProcessor {
    pub detector: EnvelopeDetector,
    /// Threshold in linear amplitude.
    threshold_linear: f64,
    /// Compression ratio (e.g. 4.0 means 4:1). Use f64::INFINITY for limiter.
    ratio: f64,
    /// Knee width in dB (0 = hard knee).
    knee_db: f64,
    /// Makeup gain in linear.
    makeup_linear: f64,
    /// If true, this is a downward gate/expander (attenuate below threshold).
    gate_mode: bool,
}

impl DynamicsProcessor {
    /// Create a compressor/limiter.
    ///
    /// - `threshold_db`: level above which compression begins
    /// - `ratio`: compression ratio (4.0 = 4:1, f64::INFINITY = brickwall limiter)
    /// - `attack_ms`, `release_ms`: envelope timing
    /// - `knee_db`: soft knee width in dB (0 = hard knee)
    /// - `makeup_db`: post-compression gain
    /// - `sample_rate`: in Hz
    pub fn compressor(
        threshold_db: f64,
        ratio: f64,
        attack_ms: f64,
        release_ms: f64,
        knee_db: f64,
        makeup_db: f64,
        sample_rate: f64,
    ) -> Self {
        Self {
            detector: EnvelopeDetector::new(
                attack_ms,
                release_ms,
                sample_rate,
                DetectionMode::Peak,
            ),
            threshold_linear: crate::db::db_to_linear(threshold_db),
            ratio,
            knee_db,
            makeup_linear: crate::db::db_to_linear(makeup_db),
            gate_mode: false,
        }
    }

    /// Create a noise gate.
    ///
    /// - `threshold_db`: level below which audio is attenuated
    /// - `ratio`: expansion ratio (e.g. 10.0 for aggressive gating, f64::INFINITY for hard gate)
    /// - `attack_ms`: how fast the gate opens
    /// - `release_ms`: how fast the gate closes (hold time baked into release)
    pub fn gate(
        threshold_db: f64,
        ratio: f64,
        attack_ms: f64,
        release_ms: f64,
        sample_rate: f64,
    ) -> Self {
        Self {
            detector: EnvelopeDetector::new(
                attack_ms,
                release_ms,
                sample_rate,
                DetectionMode::Peak,
            ),
            threshold_linear: crate::db::db_to_linear(threshold_db),
            ratio,
            knee_db: 0.0,
            makeup_linear: 1.0,
            gate_mode: true,
        }
    }

    /// Reset the internal envelope state.
    pub fn reset(&mut self) {
        self.detector.reset();
    }

    /// Compute gain reduction for a single sample. Returns linear gain multiplier.
    #[inline]
    pub fn compute_gain(&mut self, input: f32) -> f32 {
        let env = self.detector.process_sample(input) as f64;

        let gain = if self.gate_mode {
            // Gate: attenuate when below threshold
            if env < self.threshold_linear {
                let env_db = crate::db::linear_to_db(env);
                let thresh_db = crate::db::linear_to_db(self.threshold_linear);
                let diff = thresh_db - env_db;
                let reduction = diff * (1.0 - 1.0 / self.ratio);
                crate::db::db_to_linear(-reduction)
            } else {
                1.0
            }
        } else {
            // Compressor/Limiter: attenuate when above threshold
            if env > self.threshold_linear {
                let env_db = crate::db::linear_to_db(env);
                let thresh_db = crate::db::linear_to_db(self.threshold_linear);
                let over = env_db - thresh_db;

                let compressed = if self.knee_db > 0.0 && over < self.knee_db {
                    // Soft knee region
                    let knee_factor = over / self.knee_db;
                    over * (1.0 - knee_factor * (1.0 - 1.0 / self.ratio))
                } else {
                    thresh_db + over / self.ratio - thresh_db
                };

                // Gain = difference between original and compressed
                crate::db::db_to_linear(compressed - over)
            } else {
                1.0
            }
        };

        (gain * self.makeup_linear) as f32
    }

    /// Process samples in-place: detect envelope and apply gain.
    pub fn process(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            let gain = self.compute_gain(*s);
            *s *= gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_rises_on_signal() {
        let mut env = EnvelopeDetector::new(1.0, 100.0, 44100.0, DetectionMode::Peak);

        // Feed silence then impulse
        for _ in 0..100 {
            env.process_sample(0.0);
        }
        assert!(env.level() < 0.001);

        // Feed loud signal
        for _ in 0..1000 {
            env.process_sample(1.0);
        }
        assert!(
            env.level() > 0.9,
            "Envelope should rise to ~1.0, got {}",
            env.level()
        );
    }

    #[test]
    fn test_envelope_falls_on_silence() {
        let mut env = EnvelopeDetector::new(0.1, 50.0, 44100.0, DetectionMode::Peak);

        // Charge up
        for _ in 0..2000 {
            env.process_sample(1.0);
        }
        assert!(env.level() > 0.9);

        // Release into silence
        for _ in 0..10000 {
            env.process_sample(0.0);
        }
        assert!(
            env.level() < 0.02,
            "Envelope should decay, got {}",
            env.level()
        );
    }

    #[test]
    fn test_rms_mode() {
        let mut env = EnvelopeDetector::new(1.0, 100.0, 44100.0, DetectionMode::Rms);

        // Feed constant signal
        for _ in 0..5000 {
            env.process_sample(0.5);
        }

        // RMS of constant 0.5 = 0.5
        let level = env.process_sample(0.5);
        assert!(
            (level - 0.5).abs() < 0.05,
            "RMS of constant 0.5 should be ~0.5, got {}",
            level
        );
    }

    #[test]
    fn test_compressor_attenuates_loud() {
        let mut comp = DynamicsProcessor::compressor(
            -20.0, // threshold
            4.0,   // ratio 4:1
            1.0,   // fast attack
            50.0,  // release
            0.0,   // hard knee
            0.0,   // no makeup
            44100.0,
        );

        // Feed loud signal (well above -20dB threshold)
        let mut samples = vec![1.0f32; 4000];
        comp.process(&mut samples);

        // Output should be significantly attenuated
        let last = samples[3999].abs();
        assert!(
            last < 0.5,
            "Compressed output should be < 0.5, got {}",
            last
        );
    }

    #[test]
    fn test_gate_silences_quiet() {
        let mut gate = DynamicsProcessor::gate(
            -30.0, // threshold
            100.0, // aggressive ratio
            0.1,   // instant attack
            10.0,  // short release
            44100.0,
        );

        // Feed very quiet signal (below -30dB)
        let mut samples = vec![0.001f32; 4000];
        gate.process(&mut samples);

        // Should be heavily attenuated
        let last = samples[3999].abs();
        assert!(
            last < 0.0005,
            "Gated signal should be near zero, got {}",
            last
        );
    }
}
