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
    AudioGainActor, AudioNormalizeActor, BiquadFilterActor, BrightnessContrastActor,
    BytesToStreamActor, ChromaKeyActor, CompressorActor, GrayscaleFilterActor,
    StreamBufferActor, StreamStatsActor, StreamTeeActor, StreamThrottleActor,
    StreamToBytesActor,
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

    mapping
}
