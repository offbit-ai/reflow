//! FFT / IFFT wrapper over `realfft` for real-valued audio signals.
//!
//! Provides a reusable [`StftProcessor`] for Short-Time Fourier Transform
//! with overlap-add, used by FFT, IFFT, PitchShift, TimeStretch,
//! NoiseReduction, and AudioSpectrum actors.

use realfft::{RealFftPlanner, RealToComplex, ComplexToReal};
use rustfft::num_complex::Complex;
use std::sync::Arc;

use crate::ring_buffer::RingBuffer;
use crate::window::{self, WindowType};

/// A single FFT frame: frequency-domain representation.
///
/// For a window of size N, there are N/2 + 1 complex bins.
/// Bin 0 = DC, bin N/2 = Nyquist.
pub struct FftFrame {
    pub bins: Vec<Complex<f32>>,
    /// Window size this frame was computed from.
    pub window_size: usize,
}

impl FftFrame {
    /// Number of frequency bins (N/2 + 1).
    pub fn len(&self) -> usize {
        self.bins.len()
    }

    /// Get magnitude spectrum.
    pub fn magnitudes(&self) -> Vec<f32> {
        self.bins.iter().map(|c| c.norm()).collect()
    }

    /// Get phase spectrum.
    pub fn phases(&self) -> Vec<f32> {
        self.bins.iter().map(|c| c.arg()).collect()
    }

    /// Set bins from magnitude + phase.
    pub fn from_polar(magnitudes: &[f32], phases: &[f32]) -> Self {
        debug_assert_eq!(magnitudes.len(), phases.len());
        let bins: Vec<Complex<f32>> = magnitudes
            .iter()
            .zip(phases.iter())
            .map(|(&m, &p)| Complex::from_polar(m, p))
            .collect();
        let window_size = (bins.len() - 1) * 2;
        Self { bins, window_size }
    }
}

/// Short-Time Fourier Transform processor with overlap-add reconstruction.
///
/// Accumulates input samples, applies a window, performs FFT, and on the
/// inverse side performs IFFT + overlap-add to produce continuous output.
pub struct StftProcessor {
    window_size: usize,
    hop_size: usize,
    window: Vec<f32>,
    input_buf: RingBuffer,
    output_buf: Vec<f32>,
    output_pos: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    ifft: Arc<dyn ComplexToReal<f32>>,
    /// Scratch buffer for FFT input.
    fft_input: Vec<f32>,
    /// Scratch buffer for FFT output.
    fft_output: Vec<Complex<f32>>,
    /// Scratch buffer for IFFT.
    ifft_input: Vec<Complex<f32>>,
    ifft_output: Vec<f32>,
    /// Samples consumed since last FFT frame.
    samples_since_fft: usize,
    /// Whether we've accumulated enough for the first frame.
    primed: bool,
}

impl StftProcessor {
    /// Create a new STFT processor.
    ///
    /// - `window_size`: FFT size (should be power of 2 for efficiency)
    /// - `hop_size`: samples between successive FFT frames (window_size / 4 is typical 75% overlap)
    /// - `window_type`: window function to apply before FFT
    pub fn new(window_size: usize, hop_size: usize, window_type: WindowType) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(window_size);
        let ifft = planner.plan_fft_inverse(window_size);
        let bin_count = window_size / 2 + 1;

        Self {
            window_size,
            hop_size,
            window: window::generate(window_type, window_size),
            input_buf: RingBuffer::new(window_size),
            output_buf: vec![0.0; window_size * 2], // overlap-add buffer
            output_pos: 0,
            fft,
            ifft,
            fft_input: vec![0.0; window_size],
            fft_output: vec![Complex::default(); bin_count],
            ifft_input: vec![Complex::default(); bin_count],
            ifft_output: vec![0.0; window_size],
            samples_since_fft: 0,
            primed: false,
        }
    }

    pub fn window_size(&self) -> usize {
        self.window_size
    }

    pub fn hop_size(&self) -> usize {
        self.hop_size
    }

    pub fn bin_count(&self) -> usize {
        self.window_size / 2 + 1
    }

    /// Perform forward FFT on the current window contents.
    ///
    /// Returns `None` until enough samples have been accumulated.
    fn forward_fft(&mut self) -> FftFrame {
        // Copy ring buffer contents into FFT input (chronological order)
        self.input_buf.read_ordered(&mut self.fft_input);

        // Apply window
        window::apply(&mut self.fft_input, &self.window);

        // Forward FFT
        self.fft
            .process(&mut self.fft_input, &mut self.fft_output)
            .expect("FFT size mismatch");

        FftFrame {
            bins: self.fft_output.clone(),
            window_size: self.window_size,
        }
    }

    /// Perform inverse FFT and overlap-add into the output buffer.
    fn inverse_fft(&mut self, frame: &FftFrame) {
        self.ifft_input.copy_from_slice(&frame.bins);

        self.ifft
            .process(&mut self.ifft_input, &mut self.ifft_output)
            .expect("IFFT size mismatch");

        // Normalize by window_size (realfft convention)
        let norm = 1.0 / self.window_size as f32;

        // Apply synthesis window and overlap-add
        for (i, &s) in self.ifft_output.iter().enumerate() {
            let idx = (self.output_pos + i) % self.output_buf.len();
            self.output_buf[idx] += s * norm * self.window[i];
        }
    }

    /// Process input samples through a user-provided spectral transformation.
    ///
    /// `transform` receives an [`FftFrame`] and returns a (possibly modified) [`FftFrame`].
    /// Output samples are written to `output` via overlap-add.
    ///
    /// Returns the number of output samples written.
    pub fn process<F>(&mut self, input: &[f32], output: &mut Vec<f32>, transform: F)
    where
        F: Fn(FftFrame) -> FftFrame,
    {
        for &sample in input {
            self.input_buf.push(sample);
            self.samples_since_fft += 1;

            if !self.primed {
                if self.input_buf.len() >= self.window_size {
                    self.primed = true;
                    self.samples_since_fft = self.hop_size; // trigger first frame
                }
            }

            if self.primed && self.samples_since_fft >= self.hop_size {
                self.samples_since_fft = 0;

                let frame = self.forward_fft();
                let transformed = transform(frame);
                self.inverse_fft(&transformed);

                // Read hop_size samples from output buffer
                for _ in 0..self.hop_size {
                    let idx = self.output_pos % self.output_buf.len();
                    output.push(self.output_buf[idx]);
                    self.output_buf[idx] = 0.0; // clear for next overlap-add
                    self.output_pos = (self.output_pos + 1) % self.output_buf.len();
                }
            }
        }
    }

    /// Reset all internal state.
    /// Forward-only spectrum analysis: feed samples, get magnitude frames.
    ///
    /// Unlike [`process`], this does not do inverse FFT or overlap-add.
    /// Returns a `Vec<Vec<f32>>` of magnitude spectra (one per hop).
    /// Used by AudioSpectrumActor for visualization.
    pub fn analyze(&mut self, input: &[f32]) -> Vec<Vec<f32>> {
        let mut results = Vec::new();
        for &sample in input {
            self.input_buf.push(sample);
            self.samples_since_fft += 1;

            if !self.primed {
                if self.input_buf.len() >= self.window_size {
                    self.primed = true;
                    self.samples_since_fft = self.hop_size;
                }
            }

            if self.primed && self.samples_since_fft >= self.hop_size {
                self.samples_since_fft = 0;
                let frame = self.forward_fft();
                results.push(frame.magnitudes());
            }
        }
        results
    }

    pub fn reset(&mut self) {
        self.input_buf.clear();
        self.output_buf.fill(0.0);
        self.output_pos = 0;
        self.samples_since_fft = 0;
        self.primed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fft_frame_polar_roundtrip() {
        let mags = vec![1.0, 0.5, 0.25, 0.0];
        let phases = vec![0.0, 1.0, -1.0, 0.0];
        let frame = FftFrame::from_polar(&mags, &phases);

        let got_mags = frame.magnitudes();
        let got_phases = frame.phases();

        for (a, b) in mags.iter().zip(got_mags.iter()) {
            assert!((a - b).abs() < 1e-5, "{} vs {}", a, b);
        }
        for (a, b) in phases.iter().zip(got_phases.iter()) {
            assert!((a - b).abs() < 1e-5, "{} vs {}", a, b);
        }
    }

    #[test]
    fn test_stft_passthrough() {
        // Identity transform should reconstruct the input (within windowing artifacts)
        let window_size = 256;
        let hop_size = 64;
        let mut stft = StftProcessor::new(window_size, hop_size, WindowType::Hann);

        // Generate a 440Hz sine
        let n = 4096;
        let input: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 44100.0).sin() as f32)
            .collect();

        let mut output = Vec::new();
        stft.process(&input, &mut output, |frame| frame);

        // Output should be shorter (priming delay) but present
        assert!(
            output.len() > n / 2,
            "Should have produced substantial output, got {}",
            output.len()
        );

        // Check that the output is not all zeros (signal passes through)
        let rms: f32 =
            (output.iter().map(|s| s * s).sum::<f32>() / output.len() as f32).sqrt();
        assert!(rms > 0.1, "STFT passthrough should preserve signal, rms={}", rms);
    }

    #[test]
    fn test_stft_zero_bins_silences() {
        let window_size = 256;
        let hop_size = 64;
        let mut stft = StftProcessor::new(window_size, hop_size, WindowType::Hann);

        let input: Vec<f32> = (0..4096)
            .map(|i| (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 44100.0).sin() as f32)
            .collect();

        let mut output = Vec::new();
        stft.process(&input, &mut output, |mut frame| {
            // Zero out all bins
            for bin in frame.bins.iter_mut() {
                *bin = Complex::default();
            }
            frame
        });

        if !output.is_empty() {
            let rms: f32 =
                (output.iter().map(|s| s * s).sum::<f32>() / output.len() as f32).sqrt();
            assert!(rms < 0.01, "Zeroed spectrum should produce silence, rms={}", rms);
        }
    }
}
