//! Reflow actor pack: computer vision + LiteRT inference.
//!
//! Template ids are kept in sync with the `#[cfg(feature = "ml")]` arms
//! inside [`reflow_components::registry::get_actor_for_template`].

use reflow_pack_sdk::{PackHost, reflow_pack};

const TEMPLATES: &[&str] = &[
    // Computer-vision ops
    "tpl_cv_image_to_tensor",
    "tpl_cv_resize_letterbox",
    "tpl_cv_video_stream_to_frames",
    "tpl_cv_normalize_tensor",
    "tpl_cv_tensor_crop_roi",
    "tpl_cv_detection_to_roi",
    "tpl_cv_temporal_smoother",
    // Inference + post-processing
    "tpl_ml_load_model",
    "tpl_ml_run_inference",
    "tpl_ml_decode_detections",
    "tpl_ml_decode_landmarks",
    "tpl_ml_packet_probe",
];

#[reflow_pack]
fn register(host: &mut PackHost) {
    for id in TEMPLATES {
        let tid = id.to_string();
        host.register(id, move || {
            reflow_components::get_actor_for_template(&tid).unwrap_or_else(|| {
                panic!("pack did not bundle template '{tid}' — feature mismatch")
            })
        });
    }
}
