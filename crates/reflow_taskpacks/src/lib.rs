//! Reusable graph taskpacks for media/ML workflows.

use reflow_graph::types::{GraphConnection, GraphEdge, GraphExport, GraphNode, PortType};
use serde_json::{json, Value};
use std::collections::HashMap;

pub const TPL_CV_IMAGE_TO_TENSOR: &str = "tpl_cv_image_to_tensor";
pub const TPL_CV_RESIZE_LETTERBOX: &str = "tpl_cv_resize_letterbox";
pub const TPL_CV_VIDEO_STREAM_TO_FRAMES: &str = "tpl_cv_video_stream_to_frames";
pub const TPL_CV_NORMALIZE_TENSOR: &str = "tpl_cv_normalize_tensor";
pub const TPL_CV_TENSOR_CROP_ROI: &str = "tpl_cv_tensor_crop_roi";
pub const TPL_CV_DETECTION_TO_ROI: &str = "tpl_cv_detection_to_roi";
pub const TPL_CV_TEMPORAL_SMOOTHER: &str = "tpl_cv_temporal_smoother";

pub const TPL_ML_LOAD_MODEL: &str = "tpl_ml_load_model";
pub const TPL_ML_RUN_INFERENCE: &str = "tpl_ml_run_inference";
pub const TPL_ML_DECODE_DETECTIONS: &str = "tpl_ml_decode_detections";
pub const TPL_ML_DECODE_LANDMARKS: &str = "tpl_ml_decode_landmarks";
pub const TPL_ML_PACKET_PROBE: &str = "tpl_ml_packet_probe";

/// A hand-landmark-style task graph using generic CV and ML actors.
///
/// This is an authoring convenience and test fixture, not privileged runtime
/// behavior. Model details stay in actor config and can be swapped by graph
/// editors or manifests.
pub fn hand_landmark_graph() -> GraphExport {
    let mut processes = HashMap::new();
    processes.insert(
        "palm_letterbox".to_string(),
        node(
            "palm_letterbox",
            TPL_CV_RESIZE_LETTERBOX,
            json!({"width": 224, "height": 224, "fill": 0}),
        ),
    );
    processes.insert(
        "palm_tensor".to_string(),
        node(
            "palm_tensor",
            TPL_CV_IMAGE_TO_TENSOR,
            json!({"name": "image", "dtype": "f32", "layout": "nhwc", "channels": 3}),
        ),
    );
    processes.insert(
        "palm_inference".to_string(),
        node(
            "palm_inference",
            TPL_ML_RUN_INFERENCE,
            json!({
                "model_id": "hand-palm-detector",
                "backend": "mock",
                "task": "palm_detection",
                "inputs": [{"name": "image", "dtype": "f32", "shape": {"dims": [1, 224, 224, 3]}}],
                "outputs": [{"name": "detections", "dtype": "f32", "shape": {"dims": [1, 5]}}]
            }),
        ),
    );
    processes.insert(
        "palm_decode".to_string(),
        node(
            "palm_decode",
            TPL_ML_DECODE_DETECTIONS,
            json!({"threshold": 0.25, "values_per_detection": 5, "fallback_detection": true}),
        ),
    );
    processes.insert(
        "roi".to_string(),
        node(
            "roi",
            TPL_CV_DETECTION_TO_ROI,
            json!({"scale": 1.8, "square": true, "fallback_center": true}),
        ),
    );
    processes.insert(
        "crop".to_string(),
        node("crop", TPL_CV_TENSOR_CROP_ROI, json!({})),
    );
    processes.insert(
        "landmark_letterbox".to_string(),
        node(
            "landmark_letterbox",
            TPL_CV_RESIZE_LETTERBOX,
            json!({"width": 224, "height": 224, "fill": 0}),
        ),
    );
    processes.insert(
        "landmark_tensor".to_string(),
        node(
            "landmark_tensor",
            TPL_CV_IMAGE_TO_TENSOR,
            json!({"name": "roi_image", "dtype": "f32", "layout": "nhwc", "channels": 3}),
        ),
    );
    processes.insert(
        "landmark_inference".to_string(),
        node(
            "landmark_inference",
            TPL_ML_RUN_INFERENCE,
            json!({
                "model_id": "hand-landmark",
                "backend": "mock",
                "task": "landmark",
                "inputs": [{"name": "roi_image", "dtype": "f32", "shape": {"dims": [1, 224, 224, 3]}}],
                "outputs": [{"name": "landmarks", "dtype": "f32", "shape": {"dims": [1, 21, 3]}}]
            }),
        ),
    );
    processes.insert(
        "landmark_decode".to_string(),
        node(
            "landmark_decode",
            TPL_ML_DECODE_LANDMARKS,
            json!({"values_per_landmark": 3, "max_landmarks": 21}),
        ),
    );
    processes.insert(
        "smooth".to_string(),
        node("smooth", TPL_CV_TEMPORAL_SMOOTHER, json!({"alpha": 0.55})),
    );

    GraphExport {
        case_sensitive: true,
        properties: HashMap::from([
            ("name".to_string(), json!("Hand Landmark Taskpack")),
            ("kind".to_string(), json!("reflow.taskpack")),
            ("task".to_string(), json!("hand_landmark")),
            ("version".to_string(), json!(1)),
        ]),
        inports: HashMap::from([("frame".to_string(), edge("palm_letterbox", "frame"))]),
        outports: HashMap::from([
            ("landmarks".to_string(), edge("smooth", "landmarks")),
            ("detections".to_string(), edge("palm_decode", "detections")),
            ("roi".to_string(), edge("roi", "roi")),
        ]),
        groups: Vec::new(),
        processes,
        connections: vec![
            conn("palm_letterbox", "frame", "palm_tensor", "frame"),
            conn("palm_tensor", "tensor", "palm_inference", "tensor"),
            conn("palm_inference", "tensor", "palm_decode", "tensor"),
            conn("palm_decode", "detections", "roi", "detections"),
            conn("palm_letterbox", "frame", "crop", "frame"),
            conn("roi", "roi", "crop", "roi"),
            conn("crop", "frame", "landmark_letterbox", "frame"),
            conn("landmark_letterbox", "frame", "landmark_tensor", "frame"),
            conn("landmark_tensor", "tensor", "landmark_inference", "tensor"),
            conn("landmark_inference", "tensor", "landmark_decode", "tensor"),
            conn("landmark_decode", "landmarks", "smooth", "landmarks"),
        ],
        graph_dependencies: Vec::new(),
        external_connections: Vec::new(),
        provided_interfaces: HashMap::new(),
        required_interfaces: HashMap::new(),
    }
}

pub fn ml_template_mapping() -> Vec<(&'static str, &'static str)> {
    vec![
        (TPL_CV_IMAGE_TO_TENSOR, "ImageToTensorActor"),
        (TPL_CV_RESIZE_LETTERBOX, "ResizeLetterboxActor"),
        (TPL_CV_VIDEO_STREAM_TO_FRAMES, "VideoStreamToFramesActor"),
        (TPL_CV_NORMALIZE_TENSOR, "NormalizeTensorActor"),
        (TPL_CV_TENSOR_CROP_ROI, "TensorCropRoiActor"),
        (TPL_CV_DETECTION_TO_ROI, "DetectionToRoiActor"),
        (TPL_CV_TEMPORAL_SMOOTHER, "TemporalSmootherActor"),
        (TPL_ML_LOAD_MODEL, "LoadModelActor"),
        (TPL_ML_RUN_INFERENCE, "RunInferenceActor"),
        (TPL_ML_DECODE_DETECTIONS, "DecodeDetectionsActor"),
        (TPL_ML_DECODE_LANDMARKS, "DecodeLandmarksActor"),
        (TPL_ML_PACKET_PROBE, "PacketProbeActor"),
    ]
}

fn node(id: &str, component: &str, metadata: Value) -> GraphNode {
    GraphNode {
        id: id.to_string(),
        component: component.to_string(),
        metadata: Some(json_to_hash(metadata)),
    }
}

fn conn(from_node: &str, from_port: &str, to_node: &str, to_port: &str) -> GraphConnection {
    GraphConnection {
        from: edge(from_node, from_port),
        to: edge(to_node, to_port),
        metadata: None,
        data: None,
    }
}

fn edge(node_id: &str, port: &str) -> GraphEdge {
    GraphEdge {
        port_name: port.to_string(),
        port_id: port.to_string(),
        node_id: node_id.to_string(),
        index: None,
        expose: false,
        data: None,
        metadata: None,
        port_type: PortType::Any,
    }
}

fn json_to_hash(value: Value) -> HashMap<String, Value> {
    value
        .as_object()
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reflow_actor::{Actor, ActorConfig};
    use reflow_cv_ops::{
        DetectionToRoiActor, ImageToTensorActor, NormalizeTensorActor, ResizeLetterboxActor,
        TemporalSmootherActor, TensorCropRoiActor,
    };
    use reflow_media_codec::{frame_to_message, value_from_message_or_packet};
    use reflow_media_types::{ImageFormat, LandmarkSet, PacketMetadata, Timestamp, VideoFrame};
    use reflow_ml_ops::{
        DecodeDetectionsActor, DecodeLandmarksActor, LoadModelActor, PacketProbeActor,
        RunInferenceActor,
    };
    use reflow_network::subgraph::SubgraphActor;
    use std::{sync::Arc, time::Duration};

    #[test]
    fn hand_landmark_graph_has_subgraph_boundary() {
        let graph = hand_landmark_graph();

        assert!(graph.inports.contains_key("frame"));
        assert!(graph.outports.contains_key("landmarks"));
        assert_eq!(
            graph.processes["palm_inference"].component,
            TPL_ML_RUN_INFERENCE
        );
    }

    #[test]
    fn hand_landmark_graph_can_build_subgraph_actor() {
        let graph = hand_landmark_graph();
        let subgraph =
            SubgraphActor::from_graph_export(&graph, taskpack_actor_templates()).unwrap();

        assert!(subgraph.inport_map().contains_key("frame"));
        assert!(subgraph.outport_map().contains_key("landmarks"));
    }

    #[tokio::test]
    async fn hand_landmark_subgraph_processes_frame() {
        let graph = hand_landmark_graph();
        let subgraph =
            SubgraphActor::from_graph_export(&graph, taskpack_actor_templates()).unwrap();
        let inport_sender = subgraph.get_inports().0;
        let outport_receiver = subgraph.get_outports().1;

        let handle = tokio::spawn(subgraph.create_process(ActorConfig::default(), None));
        let frame = sample_frame();
        inport_sender
            .send_async(HashMap::from([(
                "frame".to_string(),
                frame_to_message(&frame).unwrap(),
            )]))
            .await
            .unwrap();

        let output = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let packet = outport_receiver
                    .recv_async()
                    .await
                    .expect("subgraph outport closed before producing landmarks");
                if packet.contains_key("landmarks") {
                    break packet;
                }
            }
        })
        .await
        .expect("hand landmark taskpack did not produce landmarks");

        let landmarks: LandmarkSet =
            value_from_message_or_packet(output.get("landmarks").unwrap()).unwrap();
        assert_eq!(landmarks.landmarks.len(), 21);
        assert_eq!(
            landmarks.metadata.timestamp,
            Some(Timestamp::from_millis(42))
        );

        subgraph.shutdown();
        handle.abort();
    }

    fn taskpack_actor_templates() -> HashMap<String, Arc<dyn Actor>> {
        HashMap::from([
            (
                TPL_CV_IMAGE_TO_TENSOR.to_string(),
                Arc::new(ImageToTensorActor::new()) as Arc<dyn Actor>,
            ),
            (
                TPL_CV_RESIZE_LETTERBOX.to_string(),
                Arc::new(ResizeLetterboxActor::new()) as Arc<dyn Actor>,
            ),
            (
                TPL_CV_NORMALIZE_TENSOR.to_string(),
                Arc::new(NormalizeTensorActor::new()) as Arc<dyn Actor>,
            ),
            (
                TPL_CV_TENSOR_CROP_ROI.to_string(),
                Arc::new(TensorCropRoiActor::new()) as Arc<dyn Actor>,
            ),
            (
                TPL_CV_DETECTION_TO_ROI.to_string(),
                Arc::new(DetectionToRoiActor::new()) as Arc<dyn Actor>,
            ),
            (
                TPL_CV_TEMPORAL_SMOOTHER.to_string(),
                Arc::new(TemporalSmootherActor::new()) as Arc<dyn Actor>,
            ),
            (
                TPL_ML_LOAD_MODEL.to_string(),
                Arc::new(LoadModelActor::new()) as Arc<dyn Actor>,
            ),
            (
                TPL_ML_RUN_INFERENCE.to_string(),
                Arc::new(RunInferenceActor::new()) as Arc<dyn Actor>,
            ),
            (
                TPL_ML_DECODE_DETECTIONS.to_string(),
                Arc::new(DecodeDetectionsActor::new()) as Arc<dyn Actor>,
            ),
            (
                TPL_ML_DECODE_LANDMARKS.to_string(),
                Arc::new(DecodeLandmarksActor::new()) as Arc<dyn Actor>,
            ),
            (
                TPL_ML_PACKET_PROBE.to_string(),
                Arc::new(PacketProbeActor::new()) as Arc<dyn Actor>,
            ),
        ])
    }

    fn sample_frame() -> VideoFrame {
        let width = 32;
        let height = 24;
        let mut data = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                data.extend_from_slice(&[
                    (x * 255 / width) as u8,
                    (y * 255 / height) as u8,
                    160,
                    255,
                ]);
            }
        }

        let mut metadata = PacketMetadata::with_timestamp(Timestamp::from_millis(42));
        metadata.sequence = Some(7);
        let mut frame = VideoFrame::new(width as u32, height as u32, ImageFormat::Rgba8, data);
        frame.metadata = metadata;
        frame
    }
}
