//! Zeal Template to Actor Registry
//!
//! Maps Zeal template IDs to their corresponding native actor implementations.
//! Script templates (tpl_javascript_script, tpl_python_script, etc.) are not
//! mapped here — they are resolved via ComponentSpec::Script and deployed to dynASB.

use crate::Actor;
use std::collections::HashMap;
use std::sync::Arc;

use crate::flow_control::{ConditionalBranchActor, LoopActor, SwitchCaseActor};
use crate::integration::HttpRequestActor;
use crate::logic::RulesEngineActor;
use crate::media::{
    AudioInputActor, AudioStreamDisplayActor, ImageInputActor, ImageStreamDisplayActor,
    VideoInputActor,
};
use crate::stream_ops::{
    AudioGainActor, AudioNormalizeActor, AudioSpectrumActor, BiquadFilterActor,
    BrightnessContrastActor, BytesToStreamActor, ChromaKeyActor, CompressorActor,
    ConvolveActor, CorrelatorActor, CrossoverActor, DCOffsetActor, DeEsserActor,
    EnvelopeFollowerActor, EqualizerActor, GrayscaleFilterActor, IFFTActor,
    ImageResizeActor, LimiterActor, NoiseGateActor, NoiseReductionActor,
    PeakDetectActor, PitchShiftActor, SilenceDetectActor, StreamBufferActor,
    StreamStatsActor, StreamTeeActor, StreamThrottleActor, StreamToBytesActor,
    TimeStretchActor,
};
#[cfg(feature = "gpu")]
use crate::gpu::sdf::{
    SdfBendActor, SdfBoxActor, SdfCapsuleActor, SdfConeActor, SdfCylinderActor,
    SdfDifferenceActor, SdfDisplaceActor, SdfIntersectionActor, SdfMaterialActor,
    SdfMirrorActor, SdfPlaneActor, SdfRepeatActor, SdfRotateActor, SdfRoundActor,
    SdfScaleActor, SdfSceneActor, SdfShellActor, SdfSmoothDifferenceActor,
    SdfSmoothIntersectionActor, SdfSmoothUnionActor, SdfSphereActor, SdfTorusActor,
    SdfTranslateActor, SdfTwistActor, SdfUnionActor,
};
use crate::transform::{DataOperationsActor, DataTransformActor};

/// Get an actor instance for a given Zeal template ID
pub fn get_actor_for_template(template_id: &str) -> Option<Arc<dyn Actor>> {
    match template_id {
        // Integration
        "tpl_http_request" => Some(Arc::new(HttpRequestActor::new())),

        // Flow Control
        "tpl_if_branch" => Some(Arc::new(ConditionalBranchActor::new())),
        "tpl_switch" => Some(Arc::new(SwitchCaseActor::new())),
        "tpl_loop" => Some(Arc::new(LoopActor::new())),

        // Data Processing
        "tpl_data_transformer" => Some(Arc::new(DataTransformActor::new())),
        "tpl_data_operations" => Some(Arc::new(DataOperationsActor::new())),

        // Logic
        "tpl_rules_engine" => Some(Arc::new(RulesEngineActor::new())),

        // Media
        "tpl_image_input" => Some(Arc::new(ImageInputActor::new())),
        "tpl_audio_input" => Some(Arc::new(AudioInputActor::new())),
        "tpl_video_input" => Some(Arc::new(VideoInputActor::new())),

        // Stream Display
        "tpl_image_stream_display" => Some(Arc::new(ImageStreamDisplayActor::new())),
        "tpl_audio_stream_display" => Some(Arc::new(AudioStreamDisplayActor::new())),

        // Stream Operations
        "tpl_bytes_to_stream" => Some(Arc::new(BytesToStreamActor::new())),
        "tpl_stream_to_bytes" => Some(Arc::new(StreamToBytesActor::new())),
        "tpl_stream_tee" => Some(Arc::new(StreamTeeActor::new())),
        "tpl_stream_buffer" => Some(Arc::new(StreamBufferActor::new())),
        "tpl_stream_throttle" => Some(Arc::new(StreamThrottleActor::new())),
        "tpl_stream_stats" => Some(Arc::new(StreamStatsActor::new())),

        // Image DSP
        "tpl_grayscale_filter" => Some(Arc::new(GrayscaleFilterActor::new())),
        "tpl_brightness_contrast" => Some(Arc::new(BrightnessContrastActor::new())),
        "tpl_chroma_key" => Some(Arc::new(ChromaKeyActor::new())),

        // Audio DSP
        "tpl_audio_gain" => Some(Arc::new(AudioGainActor::new())),
        "tpl_biquad_filter" => Some(Arc::new(BiquadFilterActor::new())),
        "tpl_compressor" => Some(Arc::new(CompressorActor::new())),
        "tpl_audio_normalize" => Some(Arc::new(AudioNormalizeActor::new())),
        "tpl_noise_gate" => Some(Arc::new(NoiseGateActor::new())),
        "tpl_de_esser" => Some(Arc::new(DeEsserActor::new())),
        "tpl_audio_spectrum" => Some(Arc::new(AudioSpectrumActor::new())),
        "tpl_silence_detect" => Some(Arc::new(SilenceDetectActor::new())),

        // Audio DSP (continued)
        "tpl_equalizer" => Some(Arc::new(EqualizerActor::new())),
        "tpl_limiter" => Some(Arc::new(LimiterActor::new())),
        "tpl_dc_offset" => Some(Arc::new(DCOffsetActor::new())),
        "tpl_envelope_follower" => Some(Arc::new(EnvelopeFollowerActor::new())),
        "tpl_crossover" => Some(Arc::new(CrossoverActor::new())),
        "tpl_peak_detect" => Some(Arc::new(PeakDetectActor::new())),
        "tpl_ifft" => Some(Arc::new(IFFTActor::new())),
        "tpl_convolve" => Some(Arc::new(ConvolveActor::new())),
        "tpl_noise_reduction" => Some(Arc::new(NoiseReductionActor::new())),
        "tpl_pitch_shift" => Some(Arc::new(PitchShiftActor::new())),
        "tpl_time_stretch" => Some(Arc::new(TimeStretchActor::new())),
        "tpl_correlator" => Some(Arc::new(CorrelatorActor::new())),

        // Image DSP (continued)
        "tpl_image_resize" => Some(Arc::new(ImageResizeActor::new())),

        // GPU / SDF (feature-gated)
        #[cfg(feature = "gpu")]
        "tpl_sdf_sphere" => Some(Arc::new(SdfSphereActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_box" => Some(Arc::new(SdfBoxActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_cylinder" => Some(Arc::new(SdfCylinderActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_torus" => Some(Arc::new(SdfTorusActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_capsule" => Some(Arc::new(SdfCapsuleActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_cone" => Some(Arc::new(SdfConeActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_plane" => Some(Arc::new(SdfPlaneActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_union" => Some(Arc::new(SdfUnionActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_intersection" => Some(Arc::new(SdfIntersectionActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_difference" => Some(Arc::new(SdfDifferenceActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_smooth_union" => Some(Arc::new(SdfSmoothUnionActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_smooth_intersection" => Some(Arc::new(SdfSmoothIntersectionActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_smooth_difference" => Some(Arc::new(SdfSmoothDifferenceActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_translate" => Some(Arc::new(SdfTranslateActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_rotate" => Some(Arc::new(SdfRotateActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_scale" => Some(Arc::new(SdfScaleActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_twist" => Some(Arc::new(SdfTwistActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_bend" => Some(Arc::new(SdfBendActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_round" => Some(Arc::new(SdfRoundActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_shell" => Some(Arc::new(SdfShellActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_mirror" => Some(Arc::new(SdfMirrorActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_repeat" => Some(Arc::new(SdfRepeatActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_displace" => Some(Arc::new(SdfDisplaceActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_material" => Some(Arc::new(SdfMaterialActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_scene" => Some(Arc::new(SdfSceneActor::new())),

        // Fall through to generated API actors (api_slack_send_message, etc.)
        #[cfg(feature = "api")]
        other => crate::api::api_registry::get_api_actor_for_template(other),
        #[cfg(not(feature = "api"))]
        _ => None,
    }
}

/// Get the complete mapping of template IDs to actor names
pub fn get_template_mapping() -> HashMap<String, String> {
    let mut mapping = HashMap::new();

    mapping.insert(
        "tpl_http_request".to_string(),
        "HttpRequestActor".to_string(),
    );
    mapping.insert(
        "tpl_if_branch".to_string(),
        "ConditionalBranchActor".to_string(),
    );
    mapping.insert("tpl_switch".to_string(), "SwitchCaseActor".to_string());
    mapping.insert("tpl_loop".to_string(), "LoopActor".to_string());
    mapping.insert(
        "tpl_data_transformer".to_string(),
        "DataTransformActor".to_string(),
    );
    mapping.insert(
        "tpl_data_operations".to_string(),
        "DataOperationsActor".to_string(),
    );
    mapping.insert(
        "tpl_rules_engine".to_string(),
        "RulesEngineActor".to_string(),
    );
    mapping.insert("tpl_image_input".to_string(), "ImageInputActor".to_string());
    mapping.insert("tpl_audio_input".to_string(), "AudioInputActor".to_string());
    mapping.insert("tpl_video_input".to_string(), "VideoInputActor".to_string());
    mapping.insert(
        "tpl_image_stream_display".to_string(),
        "ImageStreamDisplayActor".to_string(),
    );
    mapping.insert(
        "tpl_audio_stream_display".to_string(),
        "AudioStreamDisplayActor".to_string(),
    );

    // Stream Operations
    mapping.insert("tpl_bytes_to_stream".to_string(), "BytesToStreamActor".to_string());
    mapping.insert("tpl_stream_to_bytes".to_string(), "StreamToBytesActor".to_string());
    mapping.insert("tpl_stream_tee".to_string(), "StreamTeeActor".to_string());
    mapping.insert("tpl_stream_buffer".to_string(), "StreamBufferActor".to_string());
    mapping.insert("tpl_stream_throttle".to_string(), "StreamThrottleActor".to_string());
    mapping.insert("tpl_stream_stats".to_string(), "StreamStatsActor".to_string());

    // Image DSP
    mapping.insert("tpl_grayscale_filter".to_string(), "GrayscaleFilterActor".to_string());
    mapping.insert("tpl_brightness_contrast".to_string(), "BrightnessContrastActor".to_string());
    mapping.insert("tpl_chroma_key".to_string(), "ChromaKeyActor".to_string());

    // Audio DSP
    mapping.insert("tpl_audio_gain".to_string(), "AudioGainActor".to_string());
    mapping.insert("tpl_biquad_filter".to_string(), "BiquadFilterActor".to_string());
    mapping.insert("tpl_compressor".to_string(), "CompressorActor".to_string());
    mapping.insert("tpl_audio_normalize".to_string(), "AudioNormalizeActor".to_string());
    mapping.insert("tpl_noise_gate".to_string(), "NoiseGateActor".to_string());
    mapping.insert("tpl_de_esser".to_string(), "DeEsserActor".to_string());
    mapping.insert("tpl_audio_spectrum".to_string(), "AudioSpectrumActor".to_string());
    mapping.insert("tpl_silence_detect".to_string(), "SilenceDetectActor".to_string());

    // Image DSP (continued)
    mapping.insert("tpl_image_resize".to_string(), "ImageResizeActor".to_string());

    // GPU / SDF (feature-gated)
    #[cfg(feature = "gpu")]
    {
        for (id, name) in [
            ("tpl_sdf_sphere", "SdfSphereActor"), ("tpl_sdf_box", "SdfBoxActor"),
            ("tpl_sdf_cylinder", "SdfCylinderActor"), ("tpl_sdf_torus", "SdfTorusActor"),
            ("tpl_sdf_capsule", "SdfCapsuleActor"), ("tpl_sdf_cone", "SdfConeActor"),
            ("tpl_sdf_plane", "SdfPlaneActor"), ("tpl_sdf_union", "SdfUnionActor"),
            ("tpl_sdf_intersection", "SdfIntersectionActor"), ("tpl_sdf_difference", "SdfDifferenceActor"),
            ("tpl_sdf_smooth_union", "SdfSmoothUnionActor"),
            ("tpl_sdf_smooth_intersection", "SdfSmoothIntersectionActor"),
            ("tpl_sdf_smooth_difference", "SdfSmoothDifferenceActor"),
            ("tpl_sdf_translate", "SdfTranslateActor"), ("tpl_sdf_rotate", "SdfRotateActor"),
            ("tpl_sdf_scale", "SdfScaleActor"), ("tpl_sdf_twist", "SdfTwistActor"),
            ("tpl_sdf_bend", "SdfBendActor"), ("tpl_sdf_round", "SdfRoundActor"),
            ("tpl_sdf_shell", "SdfShellActor"), ("tpl_sdf_mirror", "SdfMirrorActor"),
            ("tpl_sdf_repeat", "SdfRepeatActor"), ("tpl_sdf_displace", "SdfDisplaceActor"),
            ("tpl_sdf_material", "SdfMaterialActor"), ("tpl_sdf_scene", "SdfSceneActor"),
        ] {
            mapping.insert(id.to_string(), name.to_string());
        }
    }

    // Audio DSP (continued)
    mapping.insert("tpl_equalizer".to_string(), "EqualizerActor".to_string());
    mapping.insert("tpl_limiter".to_string(), "LimiterActor".to_string());
    mapping.insert("tpl_dc_offset".to_string(), "DCOffsetActor".to_string());
    mapping.insert("tpl_envelope_follower".to_string(), "EnvelopeFollowerActor".to_string());
    mapping.insert("tpl_crossover".to_string(), "CrossoverActor".to_string());
    mapping.insert("tpl_peak_detect".to_string(), "PeakDetectActor".to_string());
    mapping.insert("tpl_ifft".to_string(), "IFFTActor".to_string());
    mapping.insert("tpl_convolve".to_string(), "ConvolveActor".to_string());
    mapping.insert("tpl_noise_reduction".to_string(), "NoiseReductionActor".to_string());
    mapping.insert("tpl_pitch_shift".to_string(), "PitchShiftActor".to_string());
    mapping.insert("tpl_time_stretch".to_string(), "TimeStretchActor".to_string());
    mapping.insert("tpl_correlator".to_string(), "CorrelatorActor".to_string());

    mapping
}
