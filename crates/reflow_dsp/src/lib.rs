//! Pure-Rust DSP primitives for Reflow audio/signal processing actors.
//!
//! This crate is Wasm-safe — no system dependencies, no threads, no filesystem.
//! All math is `f32` for samples, `f64` for coefficient precision.
//!
//! # Features
//!
//! - `simd` — Enables hand-tuned SIMD paths:
//!   - **aarch64**: NEON for sample conversion, window apply, stereo interleave
//!   - **x86_64**: SSE2 for sample conversion, window apply
//!
//! # Modules
//!
//! - [`biquad`] — Second-order IIR filters (LPF, HPF, BPF, notch, EQ, shelves)
//! - [`envelope`] — Envelope detection and dynamics processing (compressor, gate)
//! - [`db`] — Decibel ↔ linear conversion
//! - [`sample`] — Sample format conversion (i16/i32/f32) and interleaving
//! - [`window`] — Window functions (Hann, Hamming, Blackman, etc.)
//! - [`ring_buffer`] — Fixed-capacity ring buffer for delay lines and windowed processing
//! - [`fft`] — STFT processor with overlap-add (wraps `rustfft`/`realfft`)

// Re-export FFT dependencies for downstream crates
pub use realfft;
pub use rustfft;

pub mod biquad;
pub mod db;
pub mod envelope;
pub mod fft;
pub mod ring_buffer;
pub mod sample;
pub mod window;

// SIMD acceleration modules (conditionally compiled)
#[cfg(all(feature = "simd", target_arch = "aarch64"))]
pub(crate) mod simd_sample_neon;

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
pub(crate) mod simd_sample_x86;
