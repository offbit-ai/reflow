// Plumbing
mod bytes_to_stream;
mod stream_buffer;
mod stream_stats;
mod stream_tee;
mod stream_throttle;
mod stream_to_bytes;

// Image codecs
mod image_decode;
mod image_encode;

// Image DSP
mod brightness_contrast;
mod chroma_key;
mod grayscale_filter;
mod image_resize;

// Audio DSP — basic
mod audio_gain;
mod audio_normalize;
mod biquad_filter;
mod compressor;
mod dc_offset;
mod de_esser;
mod equalizer;
mod limiter;
mod noise_gate;

// Audio DSP — analysis & events
mod audio_spectrum;
mod envelope_follower;
mod peak_detect;
mod silence_detect;

// Overlay
mod image_overlay;

// Video
mod frame_buffer;
mod render_frame_collector;
#[cfg(feature = "video-encode")]
mod video_encode;

// Audio DSP — spectral / advanced
mod convolve;
mod correlator;
mod crossover;
mod ifft;
mod noise_reduction;
mod pitch_shift;
mod time_stretch;

// Plumbing re-exports
pub use bytes_to_stream::BytesToStreamActor;
pub use stream_buffer::StreamBufferActor;
pub use stream_stats::StreamStatsActor;
pub use stream_tee::StreamTeeActor;
pub use stream_throttle::StreamThrottleActor;
pub use stream_to_bytes::StreamToBytesActor;

// Image codec re-exports
pub use image_decode::ImageDecodeActor;
pub use image_encode::ImageEncodeActor;

// Image DSP re-exports
pub use brightness_contrast::BrightnessContrastActor;
pub use chroma_key::ChromaKeyActor;
pub use grayscale_filter::GrayscaleFilterActor;
pub use image_resize::ImageResizeActor;

// Audio DSP re-exports
pub use audio_gain::AudioGainActor;
pub use audio_normalize::AudioNormalizeActor;
pub use audio_spectrum::AudioSpectrumActor;
pub use biquad_filter::BiquadFilterActor;
pub use compressor::CompressorActor;
pub use convolve::ConvolveActor;
pub use correlator::CorrelatorActor;
pub use crossover::CrossoverActor;
pub use dc_offset::DCOffsetActor;
pub use de_esser::DeEsserActor;
pub use envelope_follower::EnvelopeFollowerActor;
pub use equalizer::EqualizerActor;
pub use ifft::IFFTActor;
pub use limiter::LimiterActor;
pub use noise_gate::NoiseGateActor;
pub use noise_reduction::NoiseReductionActor;
pub use peak_detect::PeakDetectActor;
pub use pitch_shift::PitchShiftActor;
pub use silence_detect::SilenceDetectActor;
pub use time_stretch::TimeStretchActor;

// Overlay re-exports
pub use image_overlay::ImageOverlayActor;

// Video re-exports
pub use frame_buffer::FrameBufferActor;
pub use render_frame_collector::RenderFrameCollectorActor;
#[cfg(feature = "video-encode")]
pub use video_encode::VideoEncoderActor;
