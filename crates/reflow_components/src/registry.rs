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
use crate::media::{AudioInputActor, ImageInputActor, VideoInputActor};
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

    mapping
}
