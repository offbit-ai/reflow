//! Pure-Rust DSP primitives for Reflow audio/signal processing actors.
//!
//! This crate is Wasm-safe — no system dependencies, no threads, no filesystem.
//! All math is `f32` for samples, `f64` for coefficient precision.
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

pub mod biquad;
pub mod db;
pub mod envelope;
pub mod fft;
pub mod ring_buffer;
pub mod sample;
pub mod window;
