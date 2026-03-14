//! Zeal Template to Actor Registry
//!
//! Maps Zeal template IDs to their corresponding native actor implementations.
//! Script templates (tpl_javascript_script, tpl_python_script, etc.) are not
//! mapped here — they are resolved via ComponentSpec::Script and deployed to dynASB.

use crate::Actor;
use std::collections::HashMap;
use std::sync::Arc;

use crate::flow_control::{ConditionalBranchActor, LoopActor, SwitchCaseActor};
use crate::math::{
    MathAbsoluteActor, MathAddActor, MathAverageActor, MathClampActor, MathDivideActor,
    MathExpressionActor, MathMinMaxActor, MathModuloActor, MathMultiplyActor, MathPowerActor,
    MathRandomActor, MathRoundActor, MathSqrtActor, MathStatisticsActor, MathSubtractActor,
    MathSumActor,
    // Vector
    Vec3Actor, Vec3AddActor, Vec3SubtractActor, Vec3ScaleActor, Vec3DotActor, Vec3CrossActor,
    Vec3NormalizeActor, Vec3LengthActor, Vec3DistanceActor, Vec3LerpActor, Vec3ReflectActor,
    // Matrix
    Mat4MultiplyActor, Mat4TransformActor, Mat4IdentityActor, Mat4TranslateActor,
    Mat4ScaleActor, Mat4RotateXActor, Mat4RotateYActor, Mat4RotateZActor,
    Mat4LookAtActor, Mat4PerspectiveActor,
    // Quaternion
    QuatFromEulerActor, QuatMultiplyActor, QuatSlerpActor, QuatRotateVec3Actor,
};
use crate::integration::HttpRequestActor;
use crate::io::{FileLoadActor, FileSaveActor};
use crate::text::{DateTimeActor, JsonParserActor, RegexMatcherActor};
use crate::logic::RulesEngineActor;
use crate::media::{
    AudioInputActor, AudioStreamDisplayActor, ImageInputActor, ImageStreamDisplayActor,
    VideoInputActor,
};
use crate::stream_ops::{
    AudioGainActor, AudioNormalizeActor, AudioSpectrumActor, BiquadFilterActor,
    BrightnessContrastActor, BytesToStreamActor, ChromaKeyActor, CompressorActor,
    ImageDecodeActor, ImageEncodeActor,
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
    SdfRenderActor, SdfTranslateActor, SdfTwistActor, SdfUnionActor,
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

        // Math
        "tpl_math_add" => Some(Arc::new(MathAddActor::new())),
        "tpl_math_subtract" => Some(Arc::new(MathSubtractActor::new())),
        "tpl_math_multiply" => Some(Arc::new(MathMultiplyActor::new())),
        "tpl_math_divide" => Some(Arc::new(MathDivideActor::new())),
        "tpl_math_modulo" => Some(Arc::new(MathModuloActor::new())),
        "tpl_math_power" => Some(Arc::new(MathPowerActor::new())),
        "tpl_math_sqrt" => Some(Arc::new(MathSqrtActor::new())),
        "tpl_math_absolute" => Some(Arc::new(MathAbsoluteActor::new())),
        "tpl_math_clamp" => Some(Arc::new(MathClampActor::new())),
        "tpl_math_min_max" => Some(Arc::new(MathMinMaxActor::new())),
        "tpl_math_round" => Some(Arc::new(MathRoundActor::new())),
        "tpl_math_random" => Some(Arc::new(MathRandomActor::new())),
        "tpl_math_average" => Some(Arc::new(MathAverageActor::new())),
        "tpl_math_sum" => Some(Arc::new(MathSumActor::new())),
        "tpl_math_statistics" => Some(Arc::new(MathStatisticsActor::new())),
        "tpl_math_expression" => Some(Arc::new(MathExpressionActor::new())),

        // Vector3
        "tpl_vec3" => Some(Arc::new(Vec3Actor::new())),
        "tpl_vec3_add" => Some(Arc::new(Vec3AddActor::new())),
        "tpl_vec3_subtract" => Some(Arc::new(Vec3SubtractActor::new())),
        "tpl_vec3_scale" => Some(Arc::new(Vec3ScaleActor::new())),
        "tpl_vec3_dot" => Some(Arc::new(Vec3DotActor::new())),
        "tpl_vec3_cross" => Some(Arc::new(Vec3CrossActor::new())),
        "tpl_vec3_normalize" => Some(Arc::new(Vec3NormalizeActor::new())),
        "tpl_vec3_length" => Some(Arc::new(Vec3LengthActor::new())),
        "tpl_vec3_distance" => Some(Arc::new(Vec3DistanceActor::new())),
        "tpl_vec3_lerp" => Some(Arc::new(Vec3LerpActor::new())),
        "tpl_vec3_reflect" => Some(Arc::new(Vec3ReflectActor::new())),

        // Matrix4
        "tpl_mat4_identity" => Some(Arc::new(Mat4IdentityActor::new())),
        "tpl_mat4_multiply" => Some(Arc::new(Mat4MultiplyActor::new())),
        "tpl_mat4_transform" => Some(Arc::new(Mat4TransformActor::new())),
        "tpl_mat4_translate" => Some(Arc::new(Mat4TranslateActor::new())),
        "tpl_mat4_scale" => Some(Arc::new(Mat4ScaleActor::new())),
        "tpl_mat4_rotate_x" => Some(Arc::new(Mat4RotateXActor::new())),
        "tpl_mat4_rotate_y" => Some(Arc::new(Mat4RotateYActor::new())),
        "tpl_mat4_rotate_z" => Some(Arc::new(Mat4RotateZActor::new())),
        "tpl_mat4_look_at" => Some(Arc::new(Mat4LookAtActor::new())),
        "tpl_mat4_perspective" => Some(Arc::new(Mat4PerspectiveActor::new())),

        // Quaternion
        "tpl_quat_from_euler" => Some(Arc::new(QuatFromEulerActor::new())),
        "tpl_quat_multiply" => Some(Arc::new(QuatMultiplyActor::new())),
        "tpl_quat_slerp" => Some(Arc::new(QuatSlerpActor::new())),
        "tpl_quat_rotate_vec3" => Some(Arc::new(QuatRotateVec3Actor::new())),

        // Text / Utilities
        "tpl_json_parser" => Some(Arc::new(JsonParserActor::new())),
        "tpl_regex_matcher" => Some(Arc::new(RegexMatcherActor::new())),
        "tpl_date_time" => Some(Arc::new(DateTimeActor::new())),

        // Image Codecs
        "tpl_image_decode" => Some(Arc::new(ImageDecodeActor::new())),
        "tpl_image_encode" => Some(Arc::new(ImageEncodeActor::new())),

        // File I/O
        "tpl_file_load" => Some(Arc::new(FileLoadActor::new())),
        "tpl_file_save" => Some(Arc::new(FileSaveActor::new())),

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
        #[cfg(feature = "gpu")]
        "tpl_sdf_render" => Some(Arc::new(SdfRenderActor::new())),

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

    // Math
    for (id, name) in [
        ("tpl_math_add", "MathAddActor"), ("tpl_math_subtract", "MathSubtractActor"),
        ("tpl_math_multiply", "MathMultiplyActor"), ("tpl_math_divide", "MathDivideActor"),
        ("tpl_math_modulo", "MathModuloActor"), ("tpl_math_power", "MathPowerActor"),
        ("tpl_math_sqrt", "MathSqrtActor"), ("tpl_math_absolute", "MathAbsoluteActor"),
        ("tpl_math_clamp", "MathClampActor"), ("tpl_math_min_max", "MathMinMaxActor"),
        ("tpl_math_round", "MathRoundActor"), ("tpl_math_random", "MathRandomActor"),
        ("tpl_math_average", "MathAverageActor"), ("tpl_math_sum", "MathSumActor"),
        ("tpl_math_statistics", "MathStatisticsActor"),
        ("tpl_math_expression", "MathExpressionActor"),
    ] {
        mapping.insert(id.to_string(), name.to_string());
    }
    // Vector / Matrix / Quaternion
    for (id, name) in [
        ("tpl_vec3", "Vec3Actor"),
        ("tpl_vec3_add", "Vec3AddActor"), ("tpl_vec3_subtract", "Vec3SubtractActor"),
        ("tpl_vec3_scale", "Vec3ScaleActor"), ("tpl_vec3_dot", "Vec3DotActor"),
        ("tpl_vec3_cross", "Vec3CrossActor"), ("tpl_vec3_normalize", "Vec3NormalizeActor"),
        ("tpl_vec3_length", "Vec3LengthActor"), ("tpl_vec3_distance", "Vec3DistanceActor"),
        ("tpl_vec3_lerp", "Vec3LerpActor"), ("tpl_vec3_reflect", "Vec3ReflectActor"),
        ("tpl_mat4_identity", "Mat4IdentityActor"), ("tpl_mat4_multiply", "Mat4MultiplyActor"),
        ("tpl_mat4_transform", "Mat4TransformActor"), ("tpl_mat4_translate", "Mat4TranslateActor"),
        ("tpl_mat4_scale", "Mat4ScaleActor"), ("tpl_mat4_rotate_x", "Mat4RotateXActor"),
        ("tpl_mat4_rotate_y", "Mat4RotateYActor"), ("tpl_mat4_rotate_z", "Mat4RotateZActor"),
        ("tpl_mat4_look_at", "Mat4LookAtActor"), ("tpl_mat4_perspective", "Mat4PerspectiveActor"),
        ("tpl_quat_from_euler", "QuatFromEulerActor"), ("tpl_quat_multiply", "QuatMultiplyActor"),
        ("tpl_quat_slerp", "QuatSlerpActor"), ("tpl_quat_rotate_vec3", "QuatRotateVec3Actor"),
    ] {
        mapping.insert(id.to_string(), name.to_string());
    }

    // Text / Utilities
    mapping.insert("tpl_json_parser".to_string(), "JsonParserActor".to_string());
    mapping.insert("tpl_regex_matcher".to_string(), "RegexMatcherActor".to_string());
    mapping.insert("tpl_date_time".to_string(), "DateTimeActor".to_string());

    mapping.insert("tpl_image_decode".to_string(), "ImageDecodeActor".to_string());
    mapping.insert("tpl_image_encode".to_string(), "ImageEncodeActor".to_string());
    mapping.insert("tpl_file_load".to_string(), "FileLoadActor".to_string());
    mapping.insert("tpl_file_save".to_string(), "FileSaveActor".to_string());

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
            ("tpl_sdf_render", "SdfRenderActor"),
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
