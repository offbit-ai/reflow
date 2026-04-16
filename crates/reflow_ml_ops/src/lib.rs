//! Generic ML actors for Reflow graphs.

use actor_macro::actor;
use anyhow::{anyhow, bail, Error, Result};
use reflow_actor::{
    message::{EncodableValue, Message},
    Actor, ActorBehavior, ActorContext, Port,
};
use reflow_asset_registry::{
    load_model_asset_from_path, manifest_from_metadata, validate_model_bytes, ModelManifest,
};
use reflow_litert::{InferenceBackend, InferenceInput, MockBackend, ModelInfo, TensorSpec};
use reflow_media_codec::{
    message_to_tensor, message_to_value, tensor_summary, tensor_to_message, value_to_object_message,
};
use reflow_media_types::{
    Detection, DetectionSet, Landmark, LandmarkSet, TensorDType, TensorPacket, TensorShape,
};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};

#[actor(
    LoadModelActor,
    inports::<100>(manifest, asset_id, model_data),
    outports::<50>(model, manifest, model_data, error),
    state(MemoryState)
)]
pub async fn load_model_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let payload = context.get_payload();
    let loaded = match load_model_for_actor(payload, &config) {
        Ok(model) => model,
        Err(err) => return Ok(error_output(&err.to_string())),
    };

    let mut out = HashMap::from([("model".to_string(), value_to_object_message(&loaded.model)?)]);
    if let Some(manifest) = loaded.manifest {
        out.insert("manifest".to_string(), value_to_object_message(&manifest)?);
    }
    if let Some(model_data) = loaded.model_data {
        out.insert(
            "model_data".to_string(),
            Message::bytes((*model_data).clone()),
        );
    }
    Ok(out)
}

#[actor(
    RunInferenceActor,
    inports::<100>(tensor, model, model_data),
    outports::<50>(tensor, tensors, inference, error),
    state(MemoryState),
    await_inports(tensor)
)]
pub async fn run_inference_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let tensor = match context.get_payload().get("tensor").map(message_to_tensor) {
        Some(Ok(tensor)) => tensor,
        Some(Err(err)) => return Ok(error_output(&err.to_string())),
        None => return Ok(error_output("Expected tensor input")),
    };
    let config = context.get_config_hashmap();
    let model = match context.get_payload().get("model") {
        Some(message) => match model_info_from_message(message) {
            Ok(model) => model,
            Err(err) => return Ok(error_output(&err.to_string())),
        },
        None => match model_info_from_config(&config) {
            Ok(model) => model,
            Err(err) => return Ok(error_output(&err.to_string())),
        },
    };

    let model_data = match context
        .get_payload()
        .get("model_data")
        .map(message_to_model_data)
    {
        Some(Ok(data)) => Some(data),
        Some(Err(err)) => return Ok(error_output(&err.to_string())),
        None => None,
    };

    let backend = MockBackend::new();
    let session = match backend.load_model(model.clone(), model_data) {
        Ok(session) => session,
        Err(err) => return Ok(error_output(&err.to_string())),
    };
    let input_name = model
        .inputs
        .first()
        .map(|spec| spec.name.clone())
        .or_else(|| tensor.name.clone())
        .unwrap_or_else(|| "input".to_string());
    let output = match session.run(&[InferenceInput {
        name: input_name,
        tensor,
    }]) {
        Ok(output) => output,
        Err(err) => return Ok(error_output(&err.to_string())),
    };

    let first = output.tensors.first().cloned();
    let mut results = HashMap::new();
    if let Some(first) = first {
        results.insert("tensor".to_string(), tensor_to_message(&first)?);
    }
    results.insert(
        "tensors".to_string(),
        value_to_object_message(&output.tensors)?,
    );
    results.insert("inference".to_string(), value_to_object_message(&output)?);
    Ok(results)
}

#[derive(Debug, Clone)]
struct ActorLoadedModel {
    model: ModelInfo,
    manifest: Option<ModelManifest>,
    model_data: Option<Arc<Vec<u8>>>,
}

fn load_model_for_actor(
    payload: &HashMap<String, Message>,
    config: &HashMap<String, Value>,
) -> Result<ActorLoadedModel> {
    if let Some(asset_id) = asset_id_from_payload_or_config(payload, config) {
        let db_path = string_config_any(config, &["$db", "dbPath", "assetDbPath"], "./assets.db");
        let loaded = load_model_asset_from_path(&db_path, &asset_id)?;
        let mut model = loaded.manifest.to_model_info();
        model
            .metadata
            .entry("loadedAssetId".to_string())
            .or_insert_with(|| json!(loaded.asset_id));
        model
            .metadata
            .entry("assetDbPath".to_string())
            .or_insert_with(|| json!(db_path));

        return Ok(ActorLoadedModel {
            model,
            manifest: Some(loaded.manifest),
            model_data: Some(loaded.data),
        });
    }

    let model_data = match payload.get("model_data").map(message_to_model_data) {
        Some(Ok(data)) => Some(data),
        Some(Err(err)) => return Err(err),
        None => None,
    };

    if let Some(message) = payload.get("manifest") {
        let manifest = manifest_from_message(message)?;
        if let Some(bytes) = model_data.as_deref() {
            validate_model_bytes(&manifest, bytes)?;
        }
        return Ok(ActorLoadedModel {
            model: manifest.to_model_info(),
            manifest: Some(manifest),
            model_data,
        });
    }

    if let Some(manifest) = manifest_from_config(config)? {
        if let Some(bytes) = model_data.as_deref() {
            validate_model_bytes(&manifest, bytes)?;
        }
        return Ok(ActorLoadedModel {
            model: manifest.to_model_info(),
            manifest: Some(manifest),
            model_data,
        });
    }

    Ok(ActorLoadedModel {
        model: model_info_from_config(config)?,
        manifest: None,
        model_data,
    })
}

#[actor(
    DecodeDetectionsActor,
    inports::<100>(tensor),
    outports::<50>(detections, error),
    state(MemoryState),
    await_inports(tensor)
)]
pub async fn decode_detections_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let tensor = match context.get_payload().get("tensor").map(message_to_tensor) {
        Some(Ok(tensor)) => tensor,
        Some(Err(err)) => return Ok(error_output(&err.to_string())),
        None => return Ok(error_output("Expected tensor input")),
    };
    let config = context.get_config_hashmap();
    match decode_detections(&tensor, &config) {
        Ok(detections) => Ok([(
            "detections".to_string(),
            value_to_object_message(&detections)?,
        )]
        .into()),
        Err(err) => Ok(error_output(&err.to_string())),
    }
}

#[actor(
    DecodeLandmarksActor,
    inports::<100>(tensor),
    outports::<50>(landmarks, error),
    state(MemoryState),
    await_inports(tensor)
)]
pub async fn decode_landmarks_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let tensor = match context.get_payload().get("tensor").map(message_to_tensor) {
        Some(Ok(tensor)) => tensor,
        Some(Err(err)) => return Ok(error_output(&err.to_string())),
        None => return Ok(error_output("Expected tensor input")),
    };
    let config = context.get_config_hashmap();
    match decode_landmarks(&tensor, &config) {
        Ok(landmarks) => Ok([(
            "landmarks".to_string(),
            value_to_object_message(&landmarks)?,
        )]
        .into()),
        Err(err) => Ok(error_output(&err.to_string())),
    }
}

#[actor(
    PacketProbeActor,
    inports::<100>(input),
    outports::<50>(summary, output, error),
    state(MemoryState)
)]
pub async fn packet_probe_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let Some(input) = context.get_payload().get("input").cloned() else {
        return Ok(error_output("Expected input"));
    };
    let summary = summarize_message(&input);
    Ok([
        (
            "summary".to_string(),
            Message::object(EncodableValue::from(summary)),
        ),
        ("output".to_string(), input),
    ]
    .into())
}

fn model_info_from_message(message: &Message) -> Result<ModelInfo> {
    let value = message_to_value(message)?;
    if let Ok(model) = serde_json::from_value::<ModelInfo>(value.clone()) {
        return Ok(model);
    }
    if let Ok(manifest) = serde_json::from_value::<ModelManifest>(value.clone()) {
        return Ok(manifest.to_model_info());
    }
    let manifest = manifest_from_metadata(&value)?;
    Ok(manifest.to_model_info())
}

fn manifest_from_message(message: &Message) -> Result<ModelManifest> {
    let value = message_to_value(message)?;
    if let Ok(manifest) = serde_json::from_value::<ModelManifest>(value.clone()) {
        return Ok(manifest);
    }
    manifest_from_metadata(&value)
}

fn model_info_from_config(config: &HashMap<String, Value>) -> Result<ModelInfo> {
    if let Some(value) = config.get("model") {
        if let Ok(model) = serde_json::from_value::<ModelInfo>(value.clone()) {
            return Ok(model);
        }
        if let Ok(manifest) = serde_json::from_value::<ModelManifest>(value.clone()) {
            return Ok(manifest.to_model_info());
        }
    }

    let id = string_config(config, "model_id", "mock-model");
    let backend = string_config(config, "backend", "mock");
    let task = string_config(config, "task", "generic");
    let inputs = tensor_specs_config(config, "inputs")?;
    let outputs = tensor_specs_config(config, "outputs")?;
    Ok(ModelInfo {
        id,
        backend,
        task,
        inputs,
        outputs: if outputs.is_empty() {
            vec![TensorSpec {
                name: "output".to_string(),
                dtype: TensorDType::F32,
                shape: TensorShape::new([1, 16]),
            }]
        } else {
            outputs
        },
        metadata: HashMap::new(),
    })
}

fn manifest_from_config(config: &HashMap<String, Value>) -> Result<Option<ModelManifest>> {
    for key in ["manifest", "modelManifest"] {
        if let Some(value) = config.get(key) {
            if let Ok(manifest) = serde_json::from_value::<ModelManifest>(value.clone()) {
                return Ok(Some(manifest));
            }
            return Ok(Some(manifest_from_metadata(value)?));
        }
    }

    if let Some(value) = config.get("model") {
        if let Ok(manifest) = serde_json::from_value::<ModelManifest>(value.clone()) {
            return Ok(Some(manifest));
        }
    }

    Ok(None)
}

fn tensor_specs_config(config: &HashMap<String, Value>, key: &str) -> Result<Vec<TensorSpec>> {
    match config.get(key) {
        Some(value) => Ok(serde_json::from_value(value.clone())?),
        None => Ok(Vec::new()),
    }
}

fn asset_id_from_payload_or_config(
    payload: &HashMap<String, Message>,
    config: &HashMap<String, Value>,
) -> Option<String> {
    payload
        .get("asset_id")
        .and_then(message_to_string)
        .or_else(|| payload.get("assetId").and_then(message_to_string))
        .or_else(|| string_config_optional_any(config, &["asset_id", "assetId"]))
        .or_else(|| string_config_optional_any(config, &["model_asset_id", "modelAssetId"]))
}

fn message_to_string(message: &Message) -> Option<String> {
    match message {
        Message::String(value) => Some(value.to_string()),
        Message::Object(_) | Message::Any(_) | Message::Event(_) => message_to_value(message)
            .ok()
            .and_then(|value| value.as_str().map(ToString::to_string)),
        Message::Encoded(encoded) => Message::decode(encoded)
            .ok()
            .and_then(|message| message_to_string(&message)),
        _ => None,
    }
}

fn message_to_model_data(message: &Message) -> Result<Arc<Vec<u8>>> {
    match message {
        Message::Bytes(bytes) => Ok(bytes.clone()),
        Message::String(value) => Ok(Arc::new(value.as_bytes().to_vec())),
        Message::Encoded(encoded) => {
            let decoded = Message::decode(encoded)
                .map_err(|err| anyhow!("failed to decode model_data message: {:?}", err))?;
            message_to_model_data(&decoded)
        }
        _ => bail!("model_data must be a Bytes, String, or Encoded message"),
    }
}

fn decode_detections(
    tensor: &TensorPacket,
    config: &HashMap<String, Value>,
) -> Result<DetectionSet> {
    let values = tensor
        .as_f32_vec()
        .ok_or_else(|| anyhow!("DecodeDetections expects an f32 tensor"))?;
    let threshold = f32_config(config, "threshold", 0.3);
    let values_per_detection = usize_config(config, "values_per_detection", 5).max(5);
    let score_index = usize_config(config, "score_index", 4);
    let max_detections = usize_config(
        config,
        "max_detections",
        values
            .len()
            .checked_div(values_per_detection)
            .unwrap_or(0)
            .max(1),
    );
    let bbox_indices = usize_vec_config(config, "bbox_indices", &[0, 1, 2, 3]);
    if bbox_indices.len() < 4 {
        bail!("bbox_indices must contain at least four indices");
    }

    let mut detections = Vec::new();
    for chunk in values.chunks(values_per_detection).take(max_detections) {
        if chunk.len() < values_per_detection {
            continue;
        }
        let score = chunk.get(score_index).copied().unwrap_or(0.0);
        if score < threshold {
            continue;
        }
        let bbox = [
            chunk[bbox_indices[0]].clamp(0.0, 1.0),
            chunk[bbox_indices[1]].clamp(0.0, 1.0),
            chunk[bbox_indices[2]].abs().clamp(0.0, 1.0),
            chunk[bbox_indices[3]].abs().clamp(0.0, 1.0),
        ];
        detections.push(Detection {
            bbox,
            score,
            label: None,
            category_id: None,
            keypoints: Vec::new(),
            metadata: HashMap::new(),
        });
    }

    if detections.is_empty() && bool_config(config, "fallback_detection", true) {
        let score = values
            .iter()
            .copied()
            .fold(0.0f32, f32::max)
            .clamp(0.0, 1.0);
        detections.push(Detection {
            bbox: [0.25, 0.25, 0.5, 0.5],
            score,
            label: None,
            category_id: None,
            keypoints: Vec::new(),
            metadata: HashMap::from([("fallback".to_string(), json!(true))]),
        });
    }

    Ok(DetectionSet {
        detections,
        metadata: tensor.metadata.clone(),
    })
}

fn decode_landmarks(tensor: &TensorPacket, config: &HashMap<String, Value>) -> Result<LandmarkSet> {
    let values = tensor
        .as_f32_vec()
        .ok_or_else(|| anyhow!("DecodeLandmarks expects an f32 tensor"))?;
    let values_per_landmark = usize_config(config, "values_per_landmark", 3).max(2);
    let max_landmarks = usize_config(
        config,
        "max_landmarks",
        values.len().checked_div(values_per_landmark).unwrap_or(0),
    );
    let visibility_index = config
        .get("visibility_index")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let presence_index = config
        .get("presence_index")
        .and_then(Value::as_u64)
        .map(|value| value as usize);

    let mut landmarks = Vec::new();
    for chunk in values.chunks(values_per_landmark).take(max_landmarks) {
        if chunk.len() < 2 {
            continue;
        }
        let mut landmark = Landmark::new(
            chunk[0].clamp(0.0, 1.0),
            chunk[1].clamp(0.0, 1.0),
            chunk.get(2).copied(),
        );
        landmark.visibility = visibility_index.and_then(|idx| chunk.get(idx).copied());
        landmark.presence = presence_index.and_then(|idx| chunk.get(idx).copied());
        landmarks.push(landmark);
    }

    let mut metadata = tensor.metadata.clone();
    metadata
        .fields
        .insert("valuesPerLandmark".to_string(), json!(values_per_landmark));
    Ok(LandmarkSet {
        landmarks,
        world_landmarks: None,
        metadata,
    })
}

fn summarize_message(message: &Message) -> Value {
    if let Ok(tensor) = message_to_tensor(message) {
        return json!({"kind": "tensor", "tensor": tensor_summary(&tensor)});
    }
    match message {
        Message::Flow => json!({"kind": "flow"}),
        Message::Boolean(value) => json!({"kind": "boolean", "value": value}),
        Message::Integer(value) => json!({"kind": "integer", "value": value}),
        Message::Float(value) => json!({"kind": "float", "value": value}),
        Message::String(value) => json!({"kind": "string", "length": value.len()}),
        Message::Bytes(bytes) => json!({"kind": "bytes", "length": bytes.len()}),
        Message::StreamHandle(handle) => json!({
            "kind": "stream",
            "streamId": handle.stream_id,
            "contentType": handle.content_type,
            "sizeHint": handle.size_hint,
        }),
        Message::Object(_) => json!({"kind": "object"}),
        Message::Array(values) => json!({"kind": "array", "length": values.len()}),
        Message::Encoded(bytes) => json!({"kind": "encoded", "length": bytes.len()}),
        Message::Error(err) => json!({"kind": "error", "message": err}),
        _ => json!({"kind": "other"}),
    }
}

fn string_config(config: &HashMap<String, Value>, key: &str, default: &str) -> String {
    config
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn string_config_any(config: &HashMap<String, Value>, keys: &[&str], default: &str) -> String {
    string_config_optional_any(config, keys).unwrap_or_else(|| default.to_string())
}

fn string_config_optional_any(config: &HashMap<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        config
            .get(*key)
            .and_then(Value::as_str)
            .map(ToString::to_string)
    })
}

fn usize_config(config: &HashMap<String, Value>, key: &str, default: usize) -> usize {
    config
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
}

fn f32_config(config: &HashMap<String, Value>, key: &str, default: f32) -> f32 {
    config
        .get(key)
        .and_then(Value::as_f64)
        .map(|value| value as f32)
        .unwrap_or(default)
}

fn bool_config(config: &HashMap<String, Value>, key: &str, default: bool) -> bool {
    config.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn usize_vec_config(config: &HashMap<String, Value>, key: &str, default: &[usize]) -> Vec<usize> {
    config
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_u64)
                .map(|value| value as usize)
                .collect()
        })
        .unwrap_or_else(|| default.to_vec())
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use reflow_actor::{types::GraphNode, ActorConfig, ActorContext, ActorLoad, MemoryState};
    use reflow_asset_registry::{sha256_hex, store_model_asset_at_path};
    use std::sync::Arc;

    #[test]
    fn detection_decoder_uses_generic_tensor_layout() {
        let tensor = TensorPacket::from_f32(
            Some("detections".to_string()),
            TensorShape::new([1, 5]),
            &[0.1, 0.2, 0.3, 0.4, 0.9],
        );
        let detections = decode_detections(&tensor, &HashMap::new()).unwrap();

        assert_eq!(detections.detections.len(), 1);
        assert_eq!(detections.detections[0].score, 0.9);
    }

    #[test]
    fn landmark_decoder_reads_triplets() {
        let tensor = TensorPacket::from_f32(
            Some("landmarks".to_string()),
            TensorShape::new([1, 2, 3]),
            &[0.1, 0.2, -0.1, 0.8, 0.9, 0.2],
        );
        let landmarks = decode_landmarks(&tensor, &HashMap::new()).unwrap();

        assert_eq!(landmarks.landmarks.len(), 2);
        assert_eq!(landmarks.landmarks[1].x, 0.8);
    }

    #[test]
    fn config_model_info_has_default_output() {
        let model =
            model_info_from_config(&HashMap::from([("model_id".to_string(), json!("demo"))]))
                .unwrap();

        assert_eq!(model.id, "demo");
        assert_eq!(model.outputs[0].name, "output");
    }

    #[tokio::test]
    async fn load_model_reads_asset_manifest_and_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("assets.db").to_string_lossy().to_string();
        let model_bytes = b"mock model bytes";
        let manifest = test_manifest("asset-hand", model_bytes);
        store_model_asset_at_path(&db_path, "asset-hand:model", model_bytes, &manifest).unwrap();

        let output = load_model_actor(test_context(
            HashMap::new(),
            HashMap::from([
                ("dbPath".to_string(), json!(db_path)),
                ("asset_id".to_string(), json!("asset-hand:model")),
            ]),
        ))
        .await
        .unwrap();

        assert!(output.contains_key("model"));
        assert!(output.contains_key("manifest"));
        assert_eq!(
            output.get("model_data"),
            Some(&Message::bytes(model_bytes.to_vec()))
        );

        let model: ModelInfo =
            serde_json::from_value(message_to_value(output.get("model").unwrap()).unwrap())
                .unwrap();
        assert_eq!(model.id, "asset-hand");
        assert_eq!(model.metadata["loadedAssetId"], json!("asset-hand:model"));
    }

    #[tokio::test]
    async fn run_inference_forwards_model_bytes_to_backend() {
        let model_bytes = b"mock model bytes";
        let model = test_manifest("with-bytes", model_bytes).to_model_info();
        let tensor = TensorPacket::from_f32(
            Some("image".to_string()),
            TensorShape::new([1, 2]),
            &[0.25, 0.75],
        );

        let output = run_inference_actor(test_context(
            HashMap::from([
                ("tensor".to_string(), tensor_to_message(&tensor).unwrap()),
                (
                    "model".to_string(),
                    value_to_object_message(&model).unwrap(),
                ),
                (
                    "model_data".to_string(),
                    Message::bytes(model_bytes.to_vec()),
                ),
            ]),
            HashMap::new(),
        ))
        .await
        .unwrap();

        let inference: reflow_litert::InferenceOutput =
            serde_json::from_value(message_to_value(output.get("inference").unwrap()).unwrap())
                .unwrap();
        assert_eq!(inference.metadata["modelBytes"], json!(model_bytes.len()));
    }

    fn test_manifest(model_id: &str, model_bytes: &[u8]) -> ModelManifest {
        ModelManifest {
            model_id: model_id.to_string(),
            task_kind: "landmark".to_string(),
            backend: "mock".to_string(),
            asset_id: Some(format!("{model_id}:model")),
            input_specs: vec![TensorSpec {
                name: "image".to_string(),
                dtype: TensorDType::F32,
                shape: TensorShape::new([1, 2]),
            }],
            output_specs: vec![TensorSpec {
                name: "landmarks".to_string(),
                dtype: TensorDType::F32,
                shape: TensorShape::new([1, 6]),
            }],
            license: "MIT".to_string(),
            source_url: "https://example.test/model".to_string(),
            checksum_sha256: sha256_hex(model_bytes),
            attribution_required: false,
            tags: vec!["test".to_string()],
            metadata: HashMap::new(),
        }
    }

    fn test_context(
        payload: HashMap<String, Message>,
        config: HashMap<String, Value>,
    ) -> ActorContext {
        ActorContext::new(
            payload,
            flume::unbounded(),
            Arc::new(parking_lot::Mutex::new(MemoryState::default())),
            ActorConfig {
                node: GraphNode {
                    id: "ml_test".to_string(),
                    component: "MlTestActor".to_string(),
                    metadata: Some(config.clone()),
                },
                resolved_env: HashMap::new(),
                config,
                namespace: None,
                inport_connection_counts: HashMap::new(),
            },
            Arc::new(ActorLoad::new(0)),
        )
    }
}
